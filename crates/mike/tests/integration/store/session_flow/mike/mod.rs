#[path = "evaluations.rs"]
mod evaluations;
#[path = "memory.rs"]
mod memory;
#[path = "requests.rs"]
mod requests;

use uuid::Uuid;

use mike::store::persistence::session_flow::{FlowComponent, FlowOutcome, FlowPage, FlowRowId};

use super::support::{Fixture, ids, no_raw};

async fn lesson(f: &Fixture) -> anyhow::Result<(i64, Uuid)> {
    Ok(sqlx::query_as("INSERT INTO memory.lessons (kind_id, text, session_id)
        SELECT id, 'RAW_SENTINEL', $1 FROM memory.lesson_kinds ORDER BY id LIMIT 1 RETURNING id, uuid")
        .bind(f.id).fetch_one(f.handle.pool()).await?)
}

async fn turn(f: &Fixture) -> anyhow::Result<i64> {
    let request = f.request(None, "primary").await?;
    Ok(sqlx::query_scalar("INSERT INTO memory.operator_turns (session_id, project_id, start_request_id, end_request_id)
        VALUES ($1, $2, $3, $3) RETURNING id").bind(f.id).bind(f.project).bind(request).fetch_one(f.handle.pool()).await?)
}

async fn intervention(f: &Fixture) -> anyhow::Result<(i64, Uuid)> {
    let steerer: i64 =
        sqlx::query_scalar("SELECT id FROM steering.steerers WHERE kind = 'verifier'")
            .fetch_one(f.handle.pool())
            .await?;
    let enrolment: i64 = sqlx::query_scalar(
        "INSERT INTO steering.enrolments
        (session_id, steerer_id, mode_id, authority_id, enrolled_by)
        SELECT $1, $2, m.id, a.id, 'test' FROM steering.modes m, steering.authorities a
        WHERE m.code = 'augment' AND a.code = 'supply_context' RETURNING id",
    )
    .bind(f.id)
    .bind(steerer)
    .fetch_one(f.handle.pool())
    .await?;
    Ok(sqlx::query_as("INSERT INTO steering.interventions
        (steerer_id, enrolment_id, session_id, verdict_id, mode_id, authority_id, payload_text, verdict_json, expires_at)
        SELECT $1, $2, $3, v.id, m.id, a.id, 'RAW_SENTINEL', '{\"RAW_SENTINEL\":true}', '2026-09-10T13:00:00Z'
        FROM steering.verdicts v, steering.modes m, steering.authorities a
        WHERE v.code = 'fired' AND m.code = 'augment' AND a.code = 'supply_context' RETURNING id, uuid")
        .bind(steerer).bind(enrolment).bind(f.id).fetch_one(f.handle.pool()).await?)
}

fn table_ids(page: &FlowPage, table: &str) -> Vec<i64> {
    let mut result: Vec<_> = page
        .activities
        .iter()
        .filter(|a| a.source.table == table)
        .filter_map(|a| match a.source.row_id {
            FlowRowId::Bigint(id) => Some(id),
            _ => None,
        })
        .collect();
    result.dedup();
    result
}

#[tokio::test]
async fn session_flow_mike_empty_sources_do_not_fabricate_analyzer_execution() -> anyhow::Result<()>
{
    let f = Fixture::new().await?;
    for component in [
        FlowComponent::ManifestStages,
        FlowComponent::LessonInjections,
        FlowComponent::VerifierEvaluations,
        FlowComponent::Judgments,
        FlowComponent::LlmCalls,
        FlowComponent::Embeddings,
        FlowComponent::Consolidation,
        FlowComponent::Interventions,
    ] {
        let page = f.page(component, 1, None).await?;
        assert!(page.activities.is_empty() && page.coverage.exhausted);
        assert_eq!(page.coverage.capture_complete, None);
    }
    f.cleanup().await
}

#[tokio::test]
async fn session_flow_mike_llm_refusal_and_null_usage_are_not_network_spans() -> anyhow::Result<()>
{
    let f = Fixture::new().await?;
    let mut expected = vec![];
    for outcome in ["ok", "refused_rate", "timeout"] {
        expected.push(sqlx::query_scalar::<_, i64>("INSERT INTO llm.calls (role, model, session_id, outcome, latency_ms, request_body, created_at)
            VALUES ('steerer', 'test/model', $1, $2, 250, '{\"RAW_SENTINEL\":true}', '2026-09-10T12:00:08Z') RETURNING id")
            .bind(f.id).bind(outcome).fetch_one(f.handle.pool()).await?);
    }
    sqlx::query(
        "INSERT INTO llm.calls (role, model, outcome) VALUES ('steerer', 'test/model', 'ok')",
    )
    .execute(f.handle.pool())
    .await?;
    let page = f.page(FlowComponent::LlmCalls, 10, None).await?;
    assert_eq!(ids(&page), expected);
    assert!(page.activities.iter().all(|a| a.occurred_at.is_none()
        && a.ended_at.is_none()
        && a.metadata.input_tokens.is_none()));
    assert_eq!(page.activities[0].metadata.latency_ms, Some(250.0));
    assert_eq!(page.activities[1].metadata.latency_ms, None);
    assert_eq!(page.activities[1].outcome, Some(FlowOutcome::Skipped));
    assert!(page.coverage.best_effort);
    no_raw(&page);
    f.cleanup().await
}

#[tokio::test]
async fn session_flow_mike_large_audit_page_is_bounded_and_measured() -> anyhow::Result<()> {
    let f = Fixture::new().await?;
    for worker in 0..32 {
        f.request(Some(&format!("worker-{worker}")), "subagent")
            .await?;
    }
    sqlx::query("INSERT INTO llm.calls (role, model, session_id, outcome, request_body)
        SELECT 'steerer', 'test/model', $1, 'ok', '{\"RAW_SENTINEL\":true}'::jsonb FROM generate_series(1, 1000)")
        .bind(f.id).execute(f.handle.pool()).await?;
    let began = std::time::Instant::now();
    let page = f.page(FlowComponent::LlmCalls, 100, None).await?;
    assert_eq!(ids(&page).len(), 100);
    assert!(!page.coverage.exhausted);
    println!(
        "P05 isolated audit page: workers=32 source_rows=1000 returned=100 elapsed_ms={:.3} native_debug_bytes={}; SQL_count=4 plus transaction setup; wire bytes deferred to P12",
        began.elapsed().as_secs_f64() * 1000.0,
        format!("{page:?}").len()
    );
    let plan: serde_json::Value = sqlx::query_scalar(
        "EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)
        SELECT id FROM llm.calls WHERE session_id = $1 AND id > 0 ORDER BY id LIMIT 101",
    )
    .bind(f.id)
    .fetch_one(f.handle.pool())
    .await?;
    println!("P05 isolated audit driver EXPLAIN: {plan}");
    no_raw(&page);
    f.cleanup().await
}
