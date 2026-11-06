use crate::db_helpers;

use anyhow::Context as _;
use serde_json::Value;

use super::support::{
    ActionFixture, bootstrap_project_session, insert_action, insert_lesson, start_consolidation_run,
};

#[tokio::test]
async fn list_consolidation_runs_filters_and_paginates_with_action_counts() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, _shutdown, pool, _test_db) = proxy.into_parts();

    let project_path = "test-consolidation-runs";
    let session_id = "22222222-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    let project_id = bootstrap_project_session(&pool, project_path, session_id).await?;
    let now = chrono::Utc::now();

    let run_gc = start_consolidation_run(&pool, "gc", "schedule", "completed", now).await?;
    let run_session = start_consolidation_run(
        &pool,
        "session",
        "manual",
        "completed",
        now + chrono::Duration::seconds(1),
    )
    .await?;
    let run_running = start_consolidation_run(
        &pool,
        "nightly",
        "schedule",
        "running",
        now + chrono::Duration::seconds(2),
    )
    .await?;

    let (lesson_id, lesson_uuid) = insert_lesson(&pool, project_id, "a raw lesson", "raw").await?;
    insert_action(
        &pool,
        ActionFixture {
            run_id: run_gc,
            lesson_id,
            lesson_uuid,
            project_id: Some(project_id),
            session_uuid: Some(session_id),
            files_touched: Some(&["src/lib.rs"]),
            lesson_text: "evaluated during gc",
            action_kind: "evaluated",
            outcome: "applied",
            error: None,
            distance: None,
            threshold: None,
            counterpart_lesson_id: None,
            counterpart_uuid: None,
            counterpart_text: None,
            runners_up: serde_json::json!([]),
            created_at: now,
        },
    )
    .await?;

    let client = reqwest::Client::new();

    assert_unfiltered_list_newest_first(&client, &inspect_url, run_gc, run_running).await?;
    assert_kind_project_and_file_filters(&client, &inspect_url, project_path, run_gc, run_session)
        .await?;
    assert_pagination(&client, &inspect_url, run_session).await?;

    pool.close().await;
    Ok(())
}

async fn assert_unfiltered_list_newest_first(
    client: &reqwest::Client,
    inspect_url: &str,
    run_gc: i64,
    run_running: i64,
) -> anyhow::Result<()> {
    let resp = client
        .get(format!("{inspect_url}/api/memory/consolidation/runs"))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: Value = resp.json().await?;
    let items = body["items"].as_array().context("items must be an array")?;
    assert_eq!(body["total_count"], 3, "all three runs must be counted");
    assert_eq!(items.len(), 3);
    assert_eq!(
        items[0]["id"], run_running,
        "newest started_at (the running pass) must sort first"
    );
    assert_eq!(items[2]["id"], run_gc, "oldest started_at must sort last");
    let running_row = &items[0];
    assert_eq!(running_row["status"], "running");
    assert_eq!(running_row["finished_at"], Value::Null);
    assert_eq!(
        running_row["action_count"], 0,
        "a run with zero logged actions must still list with action_count 0"
    );
    let gc_row = &items[2];
    assert_eq!(gc_row["action_count"], 1);
    assert_eq!(gc_row["kind"], "gc");
    assert_eq!(gc_row["trigger"], "schedule");
    Ok(())
}

async fn assert_kind_project_and_file_filters(
    client: &reqwest::Client,
    inspect_url: &str,
    project_path: &str,
    run_gc: i64,
    run_session: i64,
) -> anyhow::Result<()> {
    let resp = client
        .get(format!(
            "{inspect_url}/api/memory/consolidation/runs?kind=session"
        ))
        .send()
        .await?;
    let body: Value = resp.json().await?;
    assert_eq!(body["total_count"], 1);
    assert_eq!(body["items"][0]["id"], run_session);

    let resp = client
        .get(format!(
            "{inspect_url}/api/memory/consolidation/runs?project={project_path}"
        ))
        .send()
        .await?;
    let body: Value = resp.json().await?;
    assert_eq!(
        body["total_count"], 1,
        "only run_gc has a project-scoped action row"
    );
    assert_eq!(body["items"][0]["id"], run_gc);

    let resp = client
        .get(format!(
            "{inspect_url}/api/memory/consolidation/runs?file=src/lib.rs"
        ))
        .send()
        .await?;
    let body: Value = resp.json().await?;
    assert_eq!(body["total_count"], 1);
    assert_eq!(body["items"][0]["id"], run_gc);
    Ok(())
}

async fn assert_pagination(
    client: &reqwest::Client,
    inspect_url: &str,
    run_session: i64,
) -> anyhow::Result<()> {
    let resp = client
        .get(format!(
            "{inspect_url}/api/memory/consolidation/runs?limit=1&offset=1"
        ))
        .send()
        .await?;
    let body: Value = resp.json().await?;
    assert_eq!(
        body["total_count"], 3,
        "total_count reflects the full set, not the page"
    );
    assert_eq!(
        body["items"]
            .as_array()
            .context("items must be an array")?
            .len(),
        1
    );
    assert_eq!(
        body["items"][0]["id"], run_session,
        "middle row by started_at DESC"
    );
    assert_eq!(body["limit"], 1);
    assert_eq!(body["offset"], 1);
    Ok(())
}

#[tokio::test]
async fn get_consolidation_run_returns_full_detail_or_404() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, _shutdown, pool, _test_db) = proxy.into_parts();

    let project_path = "test-consolidation-run-detail";
    let session_id = "33333333-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    let project_id = bootstrap_project_session(&pool, project_path, session_id).await?;
    let now = chrono::Utc::now();
    let run_id = start_consolidation_run(&pool, "session", "manual", "completed", now).await?;

    let (nearest_id, nearest_uuid) =
        insert_lesson(&pool, project_id, "nearest neighbour text", "daily").await?;
    let (incoming_id, incoming_uuid) =
        insert_lesson(&pool, project_id, "incoming raw lesson", "raw").await?;
    insert_action(
        &pool,
        ActionFixture {
            run_id,
            lesson_id: incoming_id,
            lesson_uuid: incoming_uuid,
            project_id: Some(project_id),
            session_uuid: Some(session_id),
            files_touched: None,
            lesson_text: "incoming raw lesson",
            action_kind: "archived_duplicate",
            outcome: "applied",
            error: None,
            distance: Some(0.05),
            threshold: Some(0.15),
            counterpart_lesson_id: Some(nearest_id),
            counterpart_uuid: Some(nearest_uuid),
            counterpart_text: Some("nearest neighbour text"),
            runners_up: serde_json::json!([{"lesson_id": nearest_id, "distance": 0.05}]),
            created_at: now,
        },
    )
    .await?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{inspect_url}/api/memory/consolidation/runs/{run_id}"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: Value = resp.json().await?;
    assert_eq!(body["run"]["id"], run_id);
    assert_eq!(body["run"]["action_count"], 1);
    let actions = body["actions"].as_array().context("actions array")?;
    assert_eq!(actions.len(), 1);
    let action = &actions[0];
    assert_eq!(
        action["lesson_text"], "incoming raw lesson",
        "full text, never sliced"
    );
    assert_eq!(action["counterpart_text"], "nearest neighbour text");
    let action_distance = action["distance"]
        .as_f64()
        .context("distance must be present")?;
    assert!(
        (action_distance - 0.05).abs() < 1e-6,
        "distance must round-trip: {action:?}"
    );
    let runners_up = action["runners_up"]
        .as_array()
        .context("runners_up array")?;
    assert_eq!(runners_up.len(), 1);
    assert_eq!(runners_up[0]["lesson_id"], nearest_id);

    let resp = client
        .get(format!(
            "{inspect_url}/api/memory/consolidation/runs/{}",
            run_id + 999_999
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);
    let body: Value = resp.json().await?;
    assert!(body["error"].is_string());

    pool.close().await;
    Ok(())
}
