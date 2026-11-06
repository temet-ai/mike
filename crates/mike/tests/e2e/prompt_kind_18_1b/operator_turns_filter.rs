use crate::db_helpers;
use serde_json::Value;

use super::support::{
    InsertRequestParams, InsertTurnParams, bootstrap_project_session, insert_classification,
    insert_request, insert_response, insert_turn,
};

#[tokio::test]
async fn operator_turns_prompt_kind_filter() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&mock.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "3a000000-0000-4000-8000-000000000001";
    let project_id = bootstrap_project_session(&pool, "pk-op-turns-filter", session_id).await?;

    let pairs = [
        ("do the work", "operator"),
        (
            "Describe your most recent action in 3-5 words: testing",
            "activity-label",
        ),
        ("fix the bug", "operator"),
    ];
    for (i, (prompt, kind)) in pairs.iter().enumerate() {
        let r = insert_request(
            &pool,
            InsertRequestParams {
                session_id,
                project_id,
                turn_origin: "primary",
                turn_type: "user_prompt",
                status: "complete",
                human_prompt: prompt,
                completed: true,
                created_at_offset_ms: (i as i64) * 1_000,
            },
        )
        .await?;
        insert_response(&pool, r, "end_turn").await?;
        insert_turn(
            &pool,
            InsertTurnParams {
                session_id,
                project_id,
                start_request_id: r,
                end_request_id: r,
                user_prompt: Some(prompt),
                outcome: "accepted",
                closed: true,
            },
        )
        .await?;
        insert_classification(&pool, r, session_id, kind).await?;
    }

    let client = reqwest::Client::new();
    let base_url = format!("{inspect_url}/api/sessions/{session_id}/operator-turns");

    let all: Vec<Value> = client.get(&base_url).send().await?.json().await?;
    assert_eq!(all.len(), 3, "default: all 3 turns");
    for t in &all {
        assert!(
            t["prompt_kind"].is_string(),
            "every turn must carry prompt_kind: {t}"
        );
    }

    let filtered: Vec<Value> = client
        .get(format!("{base_url}?prompt_kind=operator"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(
        filtered.len(),
        2,
        "prompt_kind=operator filter must return 2 operator turns, got: {filtered:?}"
    );
    for t in &filtered {
        assert_eq!(t["prompt_kind"], "operator", "filtered: {t}");
    }

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
