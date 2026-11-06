use uuid::Uuid;

use mike::store::persistence::session_flow::{FlowComponent, FlowRowId};

use super::super::support::{Fixture, ids, no_raw};
use super::{lesson, table_ids};

#[tokio::test]
async fn session_flow_mike_judgment_history_and_embedding_replacement_are_distinct()
-> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let mut expected = vec![];
    for outcome in ["success", "partial"] {
        expected.push(sqlx::query_scalar::<_, i64>("INSERT INTO session_judges.verdicts
            (session_id, outcome_id, quality_score, rationale, judge_model, rubric_version, judged_at)
            SELECT $1, id, 0.75, 'RAW_SENTINEL', 'test/model', 1, '2026-09-10T12:00:04Z'
            FROM session_judges.outcomes WHERE code = $2 RETURNING id")
            .bind(f.id).bind(outcome).fetch_one(f.handle.pool()).await?);
    }
    let first = f.page(FlowComponent::Judgments, 1, None).await?;
    let second = f
        .page(FlowComponent::Judgments, 1, first.next_cursor.as_ref())
        .await?;
    assert_eq!([ids(&first), ids(&second)].concat(), expected);
    assert_eq!(first.activities[0].metadata.quality_score, Some(0.75));
    assert_eq!(
        second.activities[0].metadata.verdict.as_deref(),
        Some("partial")
    );
    sqlx::query("INSERT INTO memory.session_embeddings (session_id, embedding, embedding_model, source, source_updated_at, embedded_at)
        VALUES ($1, array_fill(0.01::real, ARRAY[768])::vector, 'test/model', 'centroid', now(), '2026-09-10T12:00:01Z')")
        .bind(f.session).execute(f.handle.pool()).await?;
    let old = f.page(FlowComponent::Embeddings, 1, None).await?;
    sqlx::query("UPDATE memory.session_embeddings SET source = 'overview', embedded_at = '2026-09-10T12:00:06Z' WHERE session_id = $1")
        .bind(f.session).execute(f.handle.pool()).await?;
    let new = f.page(FlowComponent::Embeddings, 1, None).await?;
    assert_eq!(old.activities[0].id, new.activities[0].id);
    assert_ne!(old.activities[0].recorded_at, new.activities[0].recorded_at);
    assert_eq!(new.activities.len(), 1);
    assert!(new.coverage.latest_only && new.coverage.exhausted && new.next_cursor.is_none());
    assert_eq!(new.coverage.upper_id, Some(FlowRowId::Uuid(f.session)));
    no_raw(&first);
    no_raw(&new);
    f.cleanup().await
}

#[tokio::test]
async fn session_flow_mike_shared_run_context_and_retained_action_identity() -> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let (lesson, lesson_uuid) = lesson(&f).await?;
    let run: i64 = sqlx::query_scalar("INSERT INTO memory.consolidation_runs (run_kind_id, trigger_id, status_id, started_at, finished_at)
        VALUES (1, 1, 2, '2026-09-10T12:00:00Z', '2026-09-10T12:00:10Z') RETURNING id")
        .fetch_one(f.handle.pool()).await?;
    let zero_run: i64 = sqlx::query_scalar("INSERT INTO memory.consolidation_runs (run_kind_id, trigger_id, status_id) VALUES (1,1,1) RETURNING id")
        .fetch_one(f.handle.pool()).await?;
    let peer_uuid = Uuid::new_v4();
    let mut expected = vec![];
    for session in [f.session, peer_uuid] {
        expected.push(sqlx::query_scalar::<_, i64>("INSERT INTO memory.consolidation_actions
            (run_id, lesson_id, lesson_uuid, session_uuid, lesson_text, action_kind_id, outcome_id, created_at)
            VALUES ($1,$2,$3,$4,'RAW_SENTINEL',7,1,'2026-09-10T12:00:05Z') RETURNING id")
            .bind(run).bind(lesson).bind(lesson_uuid).bind(session).fetch_one(f.handle.pool()).await?);
    }
    let before = f.page(FlowComponent::Consolidation, 1, None).await?;
    assert_eq!(
        table_ids(&before, "memory.consolidation_actions"),
        vec![expected[0]]
    );
    assert_eq!(table_ids(&before, "memory.consolidation_runs"), vec![run]);
    assert!(
        before
            .activities
            .iter()
            .all(|a| a.metadata.run_id != Some(zero_run))
    );
    let shared: Vec<_> = before
        .activities
        .iter()
        .filter(|a| a.source.table == "memory.consolidation_runs")
        .collect();
    assert!(shared.iter().all(|a| a.source.session_id.is_none() && a.metadata.scope.as_deref() == Some("shared")));
    assert!(shared[0].ended_at.is_none() && shared[0].outcome.is_none());
    sqlx::query("DELETE FROM memory.lessons WHERE id = $1")
        .bind(lesson)
        .execute(f.handle.pool())
        .await?;
    let after = f.page(FlowComponent::Consolidation, 1, None).await?;
    assert!(
        after
            .activities
            .iter()
            .any(|a| a.source.table == "memory.consolidation_actions"
                && a.metadata.lesson_id.is_none()
                && a.metadata.lesson_uuid == Some(lesson_uuid))
    );
    no_raw(&after);
    f.cleanup().await
}
