use crate::db_helpers;

use serde_json::Value;

use super::support::{
    ActionFixture, bootstrap_project_session, insert_action, insert_lesson, start_consolidation_run,
};

#[tokio::test]
async fn list_consolidation_decisions_filters_by_project_session_file_action_kind()
-> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, _shutdown, pool, _test_db) = proxy.into_parts();

    let project_a = "test-decisions-project-a";
    let project_b = "test-decisions-project-b";
    let session_a = "44444444-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    let session_b = "55555555-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    let project_a_id = bootstrap_project_session(&pool, project_a, session_a).await?;
    let project_b_id = bootstrap_project_session(&pool, project_b, session_b).await?;
    let now = chrono::Utc::now();
    let run_id = start_consolidation_run(&pool, "nightly", "schedule", "completed", now).await?;

    let (lesson_a, uuid_a) =
        insert_lesson(&pool, project_a_id, "lesson in project a", "daily").await?;
    let (lesson_b, uuid_b) =
        insert_lesson(&pool, project_b_id, "lesson in project b", "daily").await?;

    insert_action(
        &pool,
        ActionFixture {
            run_id,
            lesson_id: lesson_a,
            lesson_uuid: uuid_a,
            project_id: Some(project_a_id),
            session_uuid: Some(session_a),
            files_touched: Some(&["src/a.rs"]),
            lesson_text: "lesson in project a",
            action_kind: "promoted_to_daily",
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
    insert_action(
        &pool,
        ActionFixture {
            run_id,
            lesson_id: lesson_b,
            lesson_uuid: uuid_b,
            project_id: Some(project_b_id),
            session_uuid: Some(session_b),
            files_touched: Some(&["src/b.rs"]),
            lesson_text: "lesson in project b",
            action_kind: "archived_expired",
            outcome: "applied",
            error: None,
            distance: None,
            threshold: None,
            counterpart_lesson_id: None,
            counterpart_uuid: None,
            counterpart_text: None,
            runners_up: serde_json::json!([]),
            created_at: now + chrono::Duration::seconds(1),
        },
    )
    .await?;

    let client = reqwest::Client::new();

    assert_unfiltered_decisions(&client, &inspect_url).await?;
    assert_project_session_file_and_kind_filters(
        &client,
        &inspect_url,
        project_a,
        session_b,
        lesson_a,
        lesson_b,
    )
    .await?;
    assert_malformed_session_filter_400(&client, &inspect_url).await?;

    pool.close().await;
    Ok(())
}

async fn assert_unfiltered_decisions(
    client: &reqwest::Client,
    inspect_url: &str,
) -> anyhow::Result<()> {
    let resp = client
        .get(format!("{inspect_url}/api/memory/decisions"))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: Value = resp.json().await?;
    assert_eq!(body["total_count"], 2);
    assert_eq!(
        body["items"][0]["lesson_text"], "lesson in project b",
        "newest created_at first"
    );
    assert_eq!(body["limit"], 50, "default limit is 50");
    Ok(())
}

async fn assert_project_session_file_and_kind_filters(
    client: &reqwest::Client,
    inspect_url: &str,
    project_a: &str,
    session_b: &str,
    lesson_a: i64,
    lesson_b: i64,
) -> anyhow::Result<()> {
    assert_single_matching_decision(
        client,
        inspect_url,
        &format!("project={project_a}"),
        lesson_a,
    )
    .await?;
    assert_single_matching_decision(
        client,
        inspect_url,
        &format!("session={session_b}"),
        lesson_b,
    )
    .await?;
    assert_single_matching_decision(client, inspect_url, "file=src/a.rs", lesson_a).await?;
    assert_single_matching_decision(
        client,
        inspect_url,
        "action_kind=archived_expired",
        lesson_b,
    )
    .await?;
    Ok(())
}

async fn assert_single_matching_decision(
    client: &reqwest::Client,
    inspect_url: &str,
    query: &str,
    expected_lesson_id: i64,
) -> anyhow::Result<()> {
    let resp = client
        .get(format!("{inspect_url}/api/memory/decisions?{query}"))
        .send()
        .await?;
    let body: Value = resp.json().await?;
    assert_eq!(
        body["total_count"], 1,
        "query {query} filtered body: {body}"
    );
    assert_eq!(body["items"][0]["lesson_id"], expected_lesson_id);
    Ok(())
}

async fn assert_malformed_session_filter_400(
    client: &reqwest::Client,
    inspect_url: &str,
) -> anyhow::Result<()> {
    let resp = client
        .get(format!(
            "{inspect_url}/api/memory/decisions?session=not-a-uuid"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);
    Ok(())
}
