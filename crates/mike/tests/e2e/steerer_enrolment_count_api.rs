use crate::db_helpers;

use serde_json::{Value, json};
use sqlx::PgPool;

async fn create_steerer(
    client: &reqwest::Client,
    inspect_url: &str,
    name: &str,
) -> anyhow::Result<i64> {
    let resp = client
        .post(format!("{inspect_url}/api/steerers"))
        .json(&json!({
            "name": name,
            "display_name": name,
            "authority": "redirect",
            "mode": "augment",
            "model": "openrouter/meta-llama/llama-3.1-8b-instruct",
        }))
        .send()
        .await?;
    let status = resp.status();
    let body: Value = resp.json().await?;
    anyhow::ensure!(
        status == 201,
        "create steerer expected 201, got {status}: {body}"
    );
    body["steerer"]["id"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("steerer.id must be an integer, got {body}"))
}

async fn insert_session(pool: &PgPool, session_uuid: &str) -> anyhow::Result<i64> {
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, NOW(), NOW())
         RETURNING id",
    )
    .bind(format!(
        "steerer-enrolment-count-test-project-{session_uuid}"
    ))
    .fetch_one(pool)
    .await?;

    let session_id: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, NOW(), NOW())
         RETURNING id",
    )
    .bind(session_uuid)
    .bind(project_id)
    .fetch_one(pool)
    .await?;

    Ok(session_id)
}

async fn insert_enrolment(
    pool: &PgPool,
    session_id: i64,
    steerer_id: i64,
    disenrolled: bool,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO steering.enrolments
            (session_id, steerer_id, mode_id, authority_id, enrolled_by, disenrolled_at)
         VALUES (
            $1, $2,
            (SELECT id FROM steering.modes WHERE code = 'augment'),
            (SELECT id FROM steering.authorities WHERE code = 'redirect'),
            'test',
            CASE WHEN $3 THEN NOW() ELSE NULL END
         )",
    )
    .bind(session_id)
    .bind(steerer_id)
    .bind(disenrolled)
    .execute(pool)
    .await?;
    Ok(())
}

async fn fetch_steerers(client: &reqwest::Client, inspect_url: &str) -> anyhow::Result<Vec<Value>> {
    let resp = client
        .get(format!("{inspect_url}/api/steerers"))
        .send()
        .await?;
    let status = resp.status();
    let body: Value = resp.json().await?;
    anyhow::ensure!(
        status == 200,
        "GET .../steerers expected 200, got {status}: {body}"
    );
    Ok(body
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("steerers response must be array, got {body}"))?
        .clone())
}

#[tokio::test]
async fn list_steerers_reports_zero_for_never_enrolled_steerer() -> anyhow::Result<()> {
    let upstream = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset — skipping \
             list_steerers_reports_zero_for_never_enrolled_steerer"
        );
        return Ok(());
    };
    let client = reqwest::Client::new();
    let run_marker = uuid::Uuid::new_v4();

    let steerer_id = create_steerer(
        &client,
        &proxy.inspect_url,
        &format!("never-enrolled-{run_marker}"),
    )
    .await?;

    let rows = fetch_steerers(&client, &proxy.inspect_url).await?;
    let row = rows
        .iter()
        .find(|r| r["id"] == steerer_id)
        .unwrap_or_else(|| panic!("steerer (id {steerer_id}) missing from response: {rows:?}"));

    assert_eq!(row["auto_enrol"], false);
    assert_eq!(
        row["active_enrolment_count"], 0,
        "a steerer with zero enrolment rows must report 0, not be absent or null: {row:?}"
    );

    proxy.shutdown.cancel();
    proxy.pool.close().await;
    proxy.test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn list_steerers_counts_active_enrolments_and_excludes_disenrolled() -> anyhow::Result<()> {
    let upstream = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset — skipping \
             list_steerers_counts_active_enrolments_and_excludes_disenrolled"
        );
        return Ok(());
    };
    let client = reqwest::Client::new();
    let run_marker = uuid::Uuid::new_v4();

    let steerer_id = create_steerer(
        &client,
        &proxy.inspect_url,
        &format!("actively-enrolled-{run_marker}"),
    )
    .await?;

    let active_session = uuid::Uuid::new_v4().to_string();
    let active_session_id = insert_session(&proxy.pool, &active_session).await?;
    insert_enrolment(&proxy.pool, active_session_id, steerer_id, false).await?;

    let disenrolled_session = uuid::Uuid::new_v4().to_string();
    let disenrolled_session_id = insert_session(&proxy.pool, &disenrolled_session).await?;
    insert_enrolment(&proxy.pool, disenrolled_session_id, steerer_id, true).await?;

    let rows = fetch_steerers(&client, &proxy.inspect_url).await?;
    let row = rows
        .iter()
        .find(|r| r["id"] == steerer_id)
        .unwrap_or_else(|| panic!("steerer (id {steerer_id}) missing from response: {rows:?}"));

    assert_eq!(
        row["active_enrolment_count"], 1,
        "one active + one disenrolled enrolment must count only the active one: {row:?}"
    );

    proxy.shutdown.cancel();
    proxy.pool.close().await;
    proxy.test_db.cleanup().await?;
    Ok(())
}
