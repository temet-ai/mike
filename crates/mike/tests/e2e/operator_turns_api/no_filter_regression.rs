use crate::db_helpers;

use super::support::{
    InsertOperatorTurnParams, InsertRequestParams, bootstrap_project_session, insert_operator_turn,
    insert_request,
};
use sqlx::PgPool;

async fn seed_five_open_turns_across_sessions(pool: &PgPool) -> anyhow::Result<()> {
    let project_id =
        bootstrap_project_session(pool, "amb-proj-a", "11000000-0000-4000-8000-000000000001")
            .await?;
    let sessions = [
        "11000000-0000-4000-8000-000000000001",
        "11000000-0000-4000-8000-000000000002",
        "11000000-0000-4000-8000-000000000003",
        "11000000-0000-4000-8000-000000000004",
        "11000000-0000-4000-8000-000000000005",
    ];

    for (i, session) in sessions.iter().enumerate() {
        if i > 0 {
            let now = chrono::Utc::now();
            sqlx::query(
                "INSERT INTO sessions.sessions
                    (session_uuid, project_id, first_seen, last_seen)
                 VALUES ($1::text::uuid, $2, $3, $3)",
            )
            .bind(session)
            .bind(project_id)
            .bind(now)
            .execute(pool)
            .await?;
        }
        let prompt = format!("prompt {i}");
        let req = insert_request(
            pool,
            InsertRequestParams {
                session_id: session,
                project_id,
                turn_origin: "primary",
                turn_type: "user_prompt",
                human_prompt: &prompt,
                created_at_offset_ms: (i as i64) * 1_000,
            },
        )
        .await?;
        insert_operator_turn(
            pool,
            InsertOperatorTurnParams {
                session_id: session,
                project_id,
                start_request_id: req,
                end_request_id: req,
                user_prompt: &prompt,
                outcome: "unknown",
                outcome_evidence: None,
            },
        )
        .await?;
    }
    Ok(())
}

async fn probe_turn_select_base(pool: &PgPool) -> anyhow::Result<()> {
    let probe_sql = "
        SELECT
            t.id AS turn_id, s.session_uuid::text AS session_id, t.project_id,
            t.start_request_id, t.end_request_id,
            COALESCE(tx.user_prompt, '')          AS user_prompt,
            COALESCE(tx.final_assistant_text, '') AS final_assistant_text,
            COALESCE(agg.tool_call_count, 0)::int      AS tool_call_count,
            COALESCE(agg.tool_error_count, 0)::int     AS tool_error_count,
            COALESCE(to_jsonb(agg.files_touched),     '[]'::jsonb) AS files_touched,
            COALESCE(to_jsonb(agg.tools_used),        '[]'::jsonb) AS tools_used,
            COALESCE(to_jsonb(agg.agents_dispatched), '[]'::jsonb) AS agents_dispatched,
            COALESCE(agg.weasel_findings_count, 0)::int     AS weasel_findings_count,
            COALESCE(agg.total_input_tokens, 0)::bigint     AS total_input_tokens,
            COALESCE(agg.total_output_tokens, 0)::bigint    AS total_output_tokens,
            -- duration_ms: was a correlated scalar subquery; now two LEFT JOINs
            -- so the planner can use hash/index joins instead of per-row scans.
            COALESCE(
                EXTRACT(EPOCH FROM (er.completed_at - sr.created_at))::bigint * 1000,
                0
            ) AS duration_ms,
            toc.code AS outcome,
            ev.last_user_message AS outcome_evidence,
            t.classified_at,
            t.created_at,
            COALESCE(t.classified_at, t.created_at) AS updated_at
          FROM memory.operator_turns t
          JOIN memory.turn_outcomes toc ON toc.id = t.outcome_id
          JOIN sessions.sessions s ON s.id = t.session_id
          LEFT JOIN mike.operator_turn_aggregates_v agg ON agg.turn_id = t.id
          LEFT JOIN mike.operator_turn_text_v       tx  ON tx.turn_id  = t.id
          LEFT JOIN requests.requests                   ev
                 ON ev.id = t.outcome_evidence_request_id
          LEFT JOIN requests.requests sr ON sr.id = t.start_request_id
          LEFT JOIN requests.requests er ON er.id = t.end_request_id
                                        AND er.completed_at IS NOT NULL
          JOIN projects.projects p ON p.id = t.project_id
         WHERE 1 = 1
         ORDER BY t.created_at DESC LIMIT $1 OFFSET $2";
    #[derive(Debug, sqlx::FromRow)]
    #[allow(dead_code)]
    struct ProbeRow {
        turn_id: i64,
        session_id: String,
        project_id: i64,
        start_request_id: i64,
        end_request_id: i64,
        user_prompt: String,
        final_assistant_text: String,
        tool_call_count: i32,
        tool_error_count: i32,
        files_touched: serde_json::Value,
        tools_used: serde_json::Value,
        agents_dispatched: serde_json::Value,
        weasel_findings_count: i32,
        total_input_tokens: i64,
        total_output_tokens: i64,
        duration_ms: i64,
        outcome: String,
        outcome_evidence: Option<String>,
        classified_at: Option<chrono::DateTime<chrono::Utc>>,
        created_at: chrono::DateTime<chrono::Utc>,
        updated_at: chrono::DateTime<chrono::Utc>,
    }
    let _raw_rows = sqlx::query_as::<_, ProbeRow>(probe_sql)
        .bind(3_i64)
        .bind(0_i64)
        .fetch_all(pool)
        .await
        .map_err(|e| anyhow::anyhow!("direct query_as probe of TURN_SELECT_BASE failed: {e}"))?;
    Ok(())
}

#[tokio::test]
async fn list_operator_turns_no_filter_returns_200_not_500() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, _shutdown, pool, test_db) = proxy.into_parts();

    seed_five_open_turns_across_sessions(&pool).await?;
    probe_turn_select_base(&pool).await?;

    let client = reqwest::Client::new();

    let url = format!("{}/api/operator-turns?limit=3", inspect_url);
    let resp = client.get(&url).send().await?;
    let status = resp.status();
    let body_text = resp.text().await?;
    assert_eq!(
        status,
        reqwest::StatusCode::OK,
        "GET /api/operator-turns?limit=3 must return 200; got {status} with body {body_text}"
    );
    let body: Vec<serde_json::Value> = serde_json::from_str(&body_text)
        .map_err(|e| anyhow::anyhow!("response body is not JSON array: {e}; body={body_text}"))?;
    assert_eq!(
        body.len(),
        3,
        "limit=3 must slice the result set to 3 rows; got {}",
        body.len()
    );

    let url = format!("{}/api/operator-turns", inspect_url);
    let resp = client.get(&url).send().await?;
    let status = resp.status();
    let body_text = resp.text().await?;
    assert_eq!(
        status,
        reqwest::StatusCode::OK,
        "GET /api/operator-turns must return 200; got {status} with body {body_text}"
    );

    test_db.cleanup().await?;
    Ok(())
}
