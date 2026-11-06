use crate::db_helpers;

use super::support::{
    InsertOperatorTurnParams, InsertRequestParams, bootstrap_project_session, insert_operator_turn,
    insert_request,
};
use sqlx::PgPool;

async fn seed_corrected_turns_in_two_projects(
    pool: &PgPool,
) -> anyhow::Result<(i64, i64, &'static str)> {
    let session_a = "cccccccc-cccc-4ccc-8ccc-cccccccccc14";
    let project_a = "am-night-watch/tools/pipeline-utilities-214a";
    let project_a_id = bootstrap_project_session(pool, project_a, session_a).await?;
    let r_a1 = insert_request(
        pool,
        InsertRequestParams {
            session_id: session_a,
            project_id: project_a_id,
            turn_origin: "primary",
            turn_type: "user_prompt",
            human_prompt: "alpha corrected",
            created_at_offset_ms: 0,
        },
    )
    .await?;
    let turn_a_id = insert_operator_turn(
        pool,
        InsertOperatorTurnParams {
            session_id: session_a,
            project_id: project_a_id,
            start_request_id: r_a1,
            end_request_id: r_a1,
            user_prompt: "alpha corrected",
            outcome: "corrected",
            outcome_evidence: Some("no, not that"),
        },
    )
    .await?;

    let session_b = "dddddddd-dddd-4ddd-8ddd-dddddddddd14";
    let project_b = "temet/temet-proxy-214b";
    let project_b_id = bootstrap_project_session(pool, project_b, session_b).await?;
    let r_b1 = insert_request(
        pool,
        InsertRequestParams {
            session_id: session_b,
            project_id: project_b_id,
            turn_origin: "primary",
            turn_type: "user_prompt",
            human_prompt: "beta corrected",
            created_at_offset_ms: 0,
        },
    )
    .await?;
    let turn_b_id = insert_operator_turn(
        pool,
        InsertOperatorTurnParams {
            session_id: session_b,
            project_id: project_b_id,
            start_request_id: r_b1,
            end_request_id: r_b1,
            user_prompt: "beta corrected",
            outcome: "corrected",
            outcome_evidence: Some("wrong again"),
        },
    )
    .await?;

    Ok((turn_a_id, turn_b_id, project_a))
}

#[tokio::test]
async fn list_corrected_turns_by_outcome_project_filter_scopes_to_matching_project_only()
-> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, _shutdown, pool, test_db) = proxy.into_parts();

    let (turn_a_id, turn_b_id, project_a) = seed_corrected_turns_in_two_projects(&pool).await?;

    let client = reqwest::Client::new();

    let url = format!(
        "{inspect_url}/api/operator-turns/by-outcome?outcome=corrected&project={project_a}&date_from=2000-01-01T00:00:00Z"
    );
    let resp = client.get(&url).send().await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: Vec<serde_json::Value> = resp.json().await?;
    assert!(
        body.iter().any(|t| t["turn_id"] == turn_a_id),
        "project_a filter must include project_a's corrected turn: {body:?}"
    );
    assert!(
        !body.iter().any(|t| t["turn_id"] == turn_b_id),
        "project_a filter must exclude project_b's corrected turn (issue #214 repro inverted): {body:?}"
    );

    let url = format!(
        "{inspect_url}/api/operator-turns/by-outcome?outcome=corrected&project=pipeline-utilities-214a&date_from=2000-01-01T00:00:00Z"
    );
    let resp = client.get(&url).send().await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: Vec<serde_json::Value> = resp.json().await?;
    assert!(
        body.iter().any(|t| t["turn_id"] == turn_a_id),
        "substring filter must still match project_a's corrected turn: {body:?}"
    );
    assert!(
        !body.iter().any(|t| t["turn_id"] == turn_b_id),
        "substring filter must not pull in project_b's turn: {body:?}"
    );

    let url = format!(
        "{inspect_url}/api/operator-turns/by-outcome?outcome=corrected&project=no-such-project-214&date_from=2000-01-01T00:00:00Z"
    );
    let resp = client.get(&url).send().await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: Vec<serde_json::Value> = resp.json().await?;
    assert!(
        !body
            .iter()
            .any(|t| t["turn_id"] == turn_a_id || t["turn_id"] == turn_b_id),
        "a non-matching project filter must return zero of our fixture turns: {body:?}"
    );

    test_db.cleanup().await?;
    Ok(())
}
