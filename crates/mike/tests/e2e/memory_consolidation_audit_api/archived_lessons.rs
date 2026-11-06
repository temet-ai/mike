use crate::db_helpers;

use serde_json::Value;

use super::support::{
    ActionFixture, archive_lesson_as_duplicate, bootstrap_project_session, insert_action,
    insert_lesson, start_consolidation_run,
};

#[tokio::test]
async fn restore_lesson_clears_duplicate_of_and_returns_long_term() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, _shutdown, pool, _test_db) = proxy.into_parts();

    let project_path = "test-restore-lesson";
    let session_id = "99999999-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    let project_id = bootstrap_project_session(&pool, project_path, session_id).await?;
    let (survivor_id, _survivor_uuid) =
        insert_lesson(&pool, project_id, "the surviving lesson", "long_term").await?;
    let (duplicate_id, _duplicate_uuid) =
        insert_lesson(&pool, project_id, "the archived duplicate", "long_term").await?;
    archive_lesson_as_duplicate(&pool, duplicate_id, survivor_id).await?;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{inspect_url}/api/lessons/{duplicate_id}/restore"))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: Value = resp.json().await?;
    assert_eq!(body["id"], duplicate_id);
    assert_eq!(body["text"], "the archived duplicate");

    let (stage, archived_at, archived_reason_id, dup_of): (
        String,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<i64>,
        Option<i64>,
    ) = sqlx::query_as(
        "SELECT ls.code, l.archived_at, l.archived_reason_id, l.duplicate_of_id
           FROM memory.lessons l JOIN memory.lesson_stages ls ON ls.id = l.stage_id
          WHERE l.id = $1",
    )
    .bind(duplicate_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(stage, "long_term");
    assert!(archived_at.is_none(), "archived_at must be cleared");
    assert!(
        archived_reason_id.is_none(),
        "archived_reason_id must be cleared"
    );
    assert!(
        dup_of.is_none(),
        "duplicate_of_id must be cleared — the archive-requires-pointer CHECK forbids it on a non-archived row"
    );

    let resp = client
        .post(format!(
            "{inspect_url}/api/lessons/{}/restore",
            duplicate_id + 999_999
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);

    let resp = client
        .post(format!("{inspect_url}/api/lessons/{duplicate_id}/restore"))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::CONFLICT);

    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn list_archived_lessons_reads_from_lessons_not_the_decision_feed() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, _shutdown, pool, _test_db) = proxy.into_parts();

    let project_path = "test-archived-lessons";
    let other_project_path = "test-archived-lessons-other";
    let session_id = "aaaaaaaa-cccc-4ccc-8ccc-cccccccccccc";
    let project_id = bootstrap_project_session(&pool, project_path, session_id).await?;
    let other_project_id = bootstrap_project_session(
        &pool,
        other_project_path,
        "bbbbbbbb-cccc-4ccc-8ccc-cccccccccccc",
    )
    .await?;

    let (active_id, with_action_id) =
        seed_active_and_archived_with_action(&pool, project_id, session_id).await?;
    let no_action_id =
        seed_archived_without_action_and_other_project(&pool, project_id, other_project_id).await?;

    let client = reqwest::Client::new();
    assert_archived_list(
        &client,
        &inspect_url,
        project_path,
        active_id,
        with_action_id,
        no_action_id,
    )
    .await?;

    pool.close().await;
    Ok(())
}

async fn seed_active_and_archived_with_action(
    pool: &sqlx::PgPool,
    project_id: i64,
    session_id: &str,
) -> anyhow::Result<(i64, i64)> {
    let (active_id, _active_uuid) =
        insert_lesson(pool, project_id, "still active", "long_term").await?;

    let (with_action_id, with_action_uuid) =
        insert_lesson(pool, project_id, "archived via consolidation", "long_term").await?;
    let now = chrono::Utc::now();
    let run_id = start_consolidation_run(pool, "gc", "schedule", "completed", now).await?;
    insert_action(
        pool,
        ActionFixture {
            run_id,
            lesson_id: with_action_id,
            lesson_uuid: with_action_uuid,
            project_id: Some(project_id),
            session_uuid: Some(session_id),
            files_touched: None,
            lesson_text: "archived via consolidation",
            action_kind: "archived_expired",
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
    sqlx::query(
        "UPDATE memory.lessons l
            SET stage_id           = archived_s.id,
                archived_at        = $2,
                archived_reason_id = ar.id
           FROM memory.lesson_stages archived_s, memory.lesson_archived_reasons ar
          WHERE archived_s.code = 'archived' AND ar.code = 'expired' AND l.id = $1",
    )
    .bind(with_action_id)
    .bind(now)
    .execute(pool)
    .await?;

    Ok((active_id, with_action_id))
}

async fn seed_archived_without_action_and_other_project(
    pool: &sqlx::PgPool,
    project_id: i64,
    other_project_id: i64,
) -> anyhow::Result<i64> {
    let now = chrono::Utc::now();
    let (no_action_id, _no_action_uuid) = insert_lesson(
        pool,
        project_id,
        "archived before the audit tables existed",
        "long_term",
    )
    .await?;
    sqlx::query(
        "UPDATE memory.lessons l
            SET stage_id           = archived_s.id,
                archived_at        = $2,
                archived_reason_id = ar.id
           FROM memory.lesson_stages archived_s, memory.lesson_archived_reasons ar
          WHERE archived_s.code = 'archived' AND ar.code = 'operator' AND l.id = $1",
    )
    .bind(no_action_id)
    .bind(now + chrono::Duration::seconds(1))
    .execute(pool)
    .await?;

    let (other_project_lesson_id, _) =
        insert_lesson(pool, other_project_id, "archived elsewhere", "long_term").await?;
    sqlx::query(
        "UPDATE memory.lessons l
            SET stage_id           = archived_s.id,
                archived_at        = NOW(),
                archived_reason_id = ar.id
           FROM memory.lesson_stages archived_s, memory.lesson_archived_reasons ar
          WHERE archived_s.code = 'archived' AND ar.code = 'expired' AND l.id = $1",
    )
    .bind(other_project_lesson_id)
    .execute(pool)
    .await?;

    Ok(no_action_id)
}

async fn assert_archived_list(
    client: &reqwest::Client,
    inspect_url: &str,
    project_path: &str,
    active_id: i64,
    with_action_id: i64,
    no_action_id: i64,
) -> anyhow::Result<()> {
    let resp = client
        .get(format!(
            "{inspect_url}/api/lessons/archived?project={project_path}"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let rows: Vec<Value> = resp.json().await?;
    assert_eq!(
        rows.len(),
        2,
        "the active lesson and the other project's archived lesson must be excluded: {rows:?}"
    );
    assert_eq!(rows[0]["id"], no_action_id);
    assert_eq!(rows[0]["text"], "archived before the audit tables existed");
    assert_eq!(rows[0]["archived_reason"], "operator");
    assert!(
        rows[0]["archived_at"].is_string(),
        "archived_at must be populated: {:?}",
        rows[0]
    );
    assert_eq!(rows[1]["id"], with_action_id);
    assert_eq!(rows[1]["archived_reason"], "expired");
    assert!(
        !rows.iter().any(|r| r["id"] == active_id),
        "the still-active lesson must not appear"
    );
    Ok(())
}

#[tokio::test]
async fn list_archived_lessons_requires_project_param() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, _shutdown, pool, _test_db) = proxy.into_parts();

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{inspect_url}/api/lessons/archived"))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);

    pool.close().await;
    Ok(())
}
