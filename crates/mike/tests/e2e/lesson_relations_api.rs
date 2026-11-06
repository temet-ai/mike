use crate::db_helpers;

use anyhow::Context as _;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

async fn bootstrap_project(pool: &PgPool, project_path: &str) -> anyhow::Result<i64> {
    let now = chrono::Utc::now();
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, $2, $2)
         RETURNING id",
    )
    .bind(project_path)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(project_id)
}

async fn insert_lesson(pool: &PgPool, project_id: i64, text: &str) -> anyhow::Result<i64> {
    let (id, _uuid): (i64, Uuid) = sqlx::query_as(
        "INSERT INTO memory.lessons (project_id, kind_id, text, stage_id, source_id)
         SELECT $1, lk.id, $2, ls.id, lsrc.id
           FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources lsrc
          WHERE lk.code = 'correction' AND ls.code = 'long_term' AND lsrc.code = 'operator'
         RETURNING id, uuid",
    )
    .bind(project_id)
    .bind(text)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

async fn insert_proposed_relation(
    pool: &PgPool,
    from_lesson_id: i64,
    to_lesson_id: i64,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.lesson_relations
            (from_lesson_id, to_lesson_id, kind_id, method_id, status_id, detector_version)
         SELECT $1, $2, rk.id, rm.id, rs.id, 'test'
           FROM memory.lesson_relation_kinds rk,
                memory.lesson_relation_methods rm,
                memory.lesson_relation_statuses rs
          WHERE rk.code = 'supersedes' AND rm.code = 'adjudicated' AND rs.code = 'proposed'
      RETURNING id",
    )
    .bind(from_lesson_id)
    .bind(to_lesson_id)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

#[tokio::test]
async fn create_lesson_with_supersedes_lands_a_confirmed_declared_relation() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, _shutdown, pool, _test_db) = proxy.into_parts();

    let project_path = "test-lesson-relations-declared";
    let project_id = bootstrap_project(&pool, project_path).await?;
    let older_id = insert_lesson(&pool, project_id, "the older lesson").await?;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{inspect_url}/api/lessons"))
        .json(&json!({
            "project_path": project_path,
            "kind": "correction",
            "text": "the newer lesson, which supersedes the older one",
            "supersedes": older_id,
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::CREATED);
    let body: Value = resp.json().await?;
    let newer_id = body["id"].as_i64().context("id must be present")?;

    let (from_id, to_id, kind, method, status): (i64, i64, String, String, String) =
        sqlx::query_as(
            "SELECT r.from_lesson_id, r.to_lesson_id, rk.code, rm.code, rs.code
               FROM memory.lesson_relations r
               JOIN memory.lesson_relation_kinds rk ON rk.id = r.kind_id
               JOIN memory.lesson_relation_methods rm ON rm.id = r.method_id
               JOIN memory.lesson_relation_statuses rs ON rs.id = r.status_id
              WHERE r.from_lesson_id = $1",
        )
        .bind(newer_id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(from_id, newer_id);
    assert_eq!(to_id, older_id);
    assert_eq!(kind, "supersedes");
    assert_eq!(method, "declared");
    assert_eq!(
        status, "confirmed",
        "the declared method's own assertion IS the decision — it lands confirmed, not proposed"
    );

    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn create_lesson_with_unknown_supersedes_id_is_rejected() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, _shutdown, pool, _test_db) = proxy.into_parts();

    let project_path = "test-lesson-relations-bad-supersedes";
    bootstrap_project(&pool, project_path).await?;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{inspect_url}/api/lessons"))
        .json(&json!({
            "project_path": project_path,
            "kind": "correction",
            "text": "this cites a lesson that does not exist",
            "supersedes": 999_999_999_i64,
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);

    let lesson_count: i64 = sqlx::query_scalar("SELECT count(*) FROM memory.lessons")
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        lesson_count, 0,
        "a bad supersedes id must reject the whole request, not create a dangling lesson"
    );

    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn list_relations_and_confirm_reject_flow() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, _shutdown, pool, _test_db) = proxy.into_parts();

    let project_path = "test-lesson-relations-list-decide";
    let project_id = bootstrap_project(&pool, project_path).await?;
    let older_id = insert_lesson(&pool, project_id, "older lesson for confirm").await?;
    let newer_id = insert_lesson(&pool, project_id, "newer lesson for confirm").await?;
    let relation_id = insert_proposed_relation(&pool, newer_id, older_id).await?;

    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{inspect_url}/api/memory/relations"))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: Value = resp.json().await?;
    let items = body["items"].as_array().context("items must be an array")?;
    let found = items
        .iter()
        .find(|item| item["id"].as_i64() == Some(relation_id))
        .with_context(|| {
            format!("the proposed relation must appear in the unfiltered list: {body}")
        })?;
    assert_eq!(found["from_lesson_id"], newer_id);
    assert_eq!(found["to_lesson_id"], older_id);
    assert_eq!(found["kind"], "supersedes");
    assert_eq!(found["status"], "proposed");

    let resp = client
        .get(format!("{inspect_url}/api/lessons/{older_id}/relations"))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let per_lesson: Value = resp.json().await?;
    let per_lesson_arr = per_lesson.as_array().context("must be a bare array")?;
    assert_eq!(per_lesson_arr.len(), 1);
    assert_eq!(per_lesson_arr[0]["id"], relation_id);

    let resp = client
        .post(format!(
            "{inspect_url}/api/memory/relations/{relation_id}/confirm"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let confirmed: Value = resp.json().await?;
    assert_eq!(confirmed["status"], "confirmed");

    let resp = client
        .post(format!(
            "{inspect_url}/api/memory/relations/{relation_id}/confirm"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::CONFLICT);

    let second_older_id = insert_lesson(&pool, project_id, "second older lesson").await?;
    let second_relation_id = insert_proposed_relation(&pool, newer_id, second_older_id).await?;
    let resp = client
        .post(format!(
            "{inspect_url}/api/memory/relations/{second_relation_id}/reject"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let rejected: Value = resp.json().await?;
    assert_eq!(rejected["status"], "rejected");

    for lesson_id in [older_id, newer_id, second_older_id] {
        let (stage, archived_at): (String, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
            "SELECT ls.code, l.archived_at
                   FROM memory.lessons l JOIN memory.lesson_stages ls ON ls.id = l.stage_id
                  WHERE l.id = $1",
        )
        .bind(lesson_id)
        .fetch_one(&pool)
        .await?;
        assert_ne!(
            stage, "archived",
            "lesson {lesson_id} must never be archived by a relation decision"
        );
        assert!(
            archived_at.is_none(),
            "lesson {lesson_id} must never get archived_at set by a relation decision"
        );
    }

    let resp = client
        .post(format!(
            "{inspect_url}/api/memory/relations/999999999/confirm"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);

    pool.close().await;
    Ok(())
}
