use crate::db_helpers;

use super::support::{
    InsertOperatorTurnParams, InsertRequestParams, bootstrap_project_session, insert_operator_turn,
    insert_request,
};
use sqlx::PgPool;

async fn seed_project_a_and_b_turns(pool: &PgPool) -> anyhow::Result<()> {
    let session_a = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    let project_a_id = bootstrap_project_session(pool, "proj-a", session_a).await?;
    let r_a1 = insert_request(
        pool,
        InsertRequestParams {
            session_id: session_a,
            project_id: project_a_id,
            turn_origin: "primary",
            turn_type: "user_prompt",
            human_prompt: "alpha",
            created_at_offset_ms: 0,
        },
    )
    .await?;
    let r_a2 = insert_request(
        pool,
        InsertRequestParams {
            session_id: session_a,
            project_id: project_a_id,
            turn_origin: "primary",
            turn_type: "user_prompt",
            human_prompt: "alpha 2",
            created_at_offset_ms: 1_000,
        },
    )
    .await?;
    insert_operator_turn(
        pool,
        InsertOperatorTurnParams {
            session_id: session_a,
            project_id: project_a_id,
            start_request_id: r_a1,
            end_request_id: r_a1,
            user_prompt: "alpha",
            outcome: "accepted",
            outcome_evidence: Some("thanks"),
        },
    )
    .await?;
    insert_operator_turn(
        pool,
        InsertOperatorTurnParams {
            session_id: session_a,
            project_id: project_a_id,
            start_request_id: r_a2,
            end_request_id: r_a2,
            user_prompt: "alpha 2",
            outcome: "shipped",
            outcome_evidence: Some("now do beta with six or more words please"),
        },
    )
    .await?;

    let session_b = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    let project_b_id = bootstrap_project_session(pool, "proj-b", session_b).await?;
    let r_b1 = insert_request(
        pool,
        InsertRequestParams {
            session_id: session_b,
            project_id: project_b_id,
            turn_origin: "primary",
            turn_type: "user_prompt",
            human_prompt: "beta",
            created_at_offset_ms: 0,
        },
    )
    .await?;
    insert_operator_turn(
        pool,
        InsertOperatorTurnParams {
            session_id: session_b,
            project_id: project_b_id,
            start_request_id: r_b1,
            end_request_id: r_b1,
            user_prompt: "beta",
            outcome: "corrected",
            outcome_evidence: Some("no"),
        },
    )
    .await?;

    Ok(())
}

#[tokio::test]
async fn operator_turns_outcome_summary_filters_by_project() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, _shutdown, pool, test_db) = proxy.into_parts();

    seed_project_a_and_b_turns(&pool).await?;

    let client = reqwest::Client::new();
    let url = format!(
        "{}/api/operator-turns/outcome-summary?project=proj-a",
        inspect_url
    );
    let resp = client.get(&url).send().await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let summary: serde_json::Value = resp.json().await?;
    let total = summary["total"].as_i64().unwrap_or(0);
    assert_eq!(
        total, 2,
        "proj-a should have exactly 2 turns, got {total} ({summary})"
    );
    let counts = summary["counts"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("counts array missing"))?;
    assert!(
        !counts.iter().any(|c| c["outcome"] == "corrected"),
        "proj-a summary must NOT include proj-b's corrected outcome: {summary}"
    );

    test_db.cleanup().await?;
    Ok(())
}
