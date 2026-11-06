use crate::db_helpers;

use super::support::{
    InsertOperatorTurnParams, InsertRequestParams, bootstrap_project_session, insert_operator_turn,
    insert_request,
};
use sqlx::PgPool;

async fn seed_recent_and_old_turns(
    pool: &PgPool,
    project_id: i64,
    session_id: &str,
) -> anyhow::Result<(i64, i64)> {
    let r_recent = insert_request(
        pool,
        InsertRequestParams {
            session_id,
            project_id,
            turn_origin: "primary",
            turn_type: "user_prompt",
            human_prompt: "recent turn",
            created_at_offset_ms: 0,
        },
    )
    .await?;
    let thirty_days_ms: i64 = -30 * 24 * 60 * 60 * 1_000;
    let r_old = insert_request(
        pool,
        InsertRequestParams {
            session_id,
            project_id,
            turn_origin: "primary",
            turn_type: "user_prompt",
            human_prompt: "old turn",
            created_at_offset_ms: thirty_days_ms,
        },
    )
    .await?;

    let recent_turn = insert_operator_turn(
        pool,
        InsertOperatorTurnParams {
            session_id,
            project_id,
            start_request_id: r_recent,
            end_request_id: r_recent,
            user_prompt: "recent turn",
            outcome: "unknown",
            outcome_evidence: None,
        },
    )
    .await?;
    let old_turn = insert_operator_turn(
        pool,
        InsertOperatorTurnParams {
            session_id,
            project_id,
            start_request_id: r_old,
            end_request_id: r_old,
            user_prompt: "old turn",
            outcome: "unknown",
            outcome_evidence: None,
        },
    )
    .await?;
    sqlx::query(
        "UPDATE memory.operator_turns SET created_at = now() - interval '30 days' WHERE id = $1",
    )
    .bind(old_turn)
    .execute(pool)
    .await?;

    Ok((recent_turn, old_turn))
}

#[tokio::test]
async fn list_operator_turns_default_window_filters_old_and_announces() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, _shutdown, pool, test_db) = proxy.into_parts();

    let project_path = "test-operator-turns-window";
    let session_id = "abcdabcd-abcd-4bcd-8bcd-abcdabcdabcd";
    let project_id = bootstrap_project_session(&pool, project_path, session_id).await?;
    let (recent_turn, old_turn) = seed_recent_and_old_turns(&pool, project_id, session_id).await?;

    let client = reqwest::Client::new();

    let url = format!(
        "{}/api/operator-turns?project={}",
        inspect_url, project_path
    );
    let resp = client.get(&url).send().await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let note = resp
        .headers()
        .get("x-window-note")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let note = note.ok_or_else(|| {
        anyhow::anyhow!("criterion 4: default window must emit X-Window-Note header")
    })?;
    assert!(
        note.contains("7 days"),
        "criterion 3: default note must name the 7-day window; got: {note:?}"
    );
    let rows: Vec<serde_json::Value> = resp.json().await?;
    let ids: Vec<i64> = rows.iter().filter_map(|r| r["turn_id"].as_i64()).collect();
    assert!(
        ids.contains(&recent_turn),
        "criterion 3: recent turn must be inside the default 7-day window; got {ids:?}"
    );
    assert!(
        !ids.contains(&old_turn),
        "criterion 3: 30-day-old turn must be EXCLUDED by the default window; got {ids:?}"
    );

    let url = format!(
        "{}/api/operator-turns?project={}&date_from=2020-01-01T00:00:00Z",
        inspect_url, project_path
    );
    let resp = client.get(&url).send().await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let rows: Vec<serde_json::Value> = resp.json().await?;
    let ids: Vec<i64> = rows.iter().filter_map(|r| r["turn_id"].as_i64()).collect();
    assert!(
        ids.contains(&recent_turn) && ids.contains(&old_turn),
        "explicit date_from must widen the window to include the old turn; got {ids:?}"
    );

    let url = format!(
        "{}/api/operator-turns?project={}&since=2020-01-01T00:00:00Z",
        inspect_url, project_path
    );
    let resp = client.get(&url).send().await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let rows: Vec<serde_json::Value> = resp.json().await?;
    let ids: Vec<i64> = rows.iter().filter_map(|r| r["turn_id"].as_i64()).collect();
    assert!(
        ids.contains(&old_turn),
        "legacy `since` must alias date_from and widen the window; got {ids:?}"
    );

    let url = format!(
        "{}/api/operator-turns?project={}&date_to=not-a-date",
        inspect_url, project_path
    );
    let resp = client.get(&url).send().await?;
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "bad date_to must return 400"
    );

    test_db.cleanup().await?;
    Ok(())
}
