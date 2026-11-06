use crate::db_helpers;

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn create_rule(
    client: &reqwest::Client,
    inspect_url: &str,
    value: &str,
) -> anyhow::Result<i64> {
    let resp = client
        .post(format!("{inspect_url}/api/filter/rules"))
        .json(&json!({"kind": "xml_tag", "value": value, "action": "drop"}))
        .send()
        .await?;
    let status = resp.status();
    let body: Value = resp.json().await?;
    anyhow::ensure!(
        status == 201,
        "create rule expected 201, got {status}: {body}"
    );
    body["rule"]["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("rule.id must be a string, got {body}"))?
        .parse::<i64>()
        .map_err(|e| anyhow::anyhow!("rule.id {:?} not a valid i64: {e}", body["rule"]["id"]))
}

async fn insert_request_with_system_prompt_at(
    pool: &PgPool,
    session_uuid: &str,
    request_id: i64,
    created_at: DateTime<Utc>,
) -> anyhow::Result<i64> {
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, NOW(), NOW())
         RETURNING id",
    )
    .bind(format!(
        "filter-rule-firing-counts-test-project-{request_id}"
    ))
    .fetch_one(pool)
    .await?;

    sqlx::query(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, NOW(), NOW())
         ON CONFLICT (session_uuid) DO NOTHING",
    )
    .bind(session_uuid)
    .bind(project_id)
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT INTO requests.requests (
            id, session_id, project_id,
            method_id, path, is_streaming, message_count, is_tool_loop,
            status_id,
            params,
            last_user_message, human_prompt,
            turn_type_id, turn_origin_id,
            created_at
         )
         OVERRIDING SYSTEM VALUE
         VALUES (
            $1, (SELECT id FROM sessions.sessions WHERE session_uuid = $2::text::uuid), $3,
            (SELECT id FROM requests.methods WHERE code = 'POST'),
            '/v1/messages', false, 1, false,
            (SELECT id FROM requests.statuses WHERE code = 'complete'),
            '{}'::jsonb,
            '', '',
            (SELECT id FROM requests.turn_types WHERE code = 'user_prompt'),
            (SELECT id FROM requests.turn_origins WHERE code = 'primary'),
            $4
         )",
    )
    .bind(request_id)
    .bind(session_uuid)
    .bind(project_id)
    .bind(created_at)
    .execute(pool)
    .await?;

    let system_prompt_id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.system_prompts (request_id, override_applied)
         VALUES ($1, false)
         RETURNING id",
    )
    .bind(request_id)
    .fetch_one(pool)
    .await?;

    Ok(system_prompt_id)
}

async fn insert_firing(pool: &PgPool, system_prompt_id: i64, rule_id: i64) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO requests.system_prompt_firing_rules
            (request_system_prompt_id, filter_rule_id, ord)
         VALUES ($1, $2, 0)",
    )
    .bind(system_prompt_id)
    .bind(rule_id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn fetch_filter_rules(
    client: &reqwest::Client,
    inspect_url: &str,
) -> anyhow::Result<Vec<Value>> {
    let resp = client
        .get(format!("{inspect_url}/api/filter/rules"))
        .send()
        .await?;
    let status = resp.status();
    let body: Value = resp.json().await?;
    anyhow::ensure!(
        status == 200,
        "GET .../filter/rules expected 200, got {status}: {body}"
    );
    Ok(body
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("filter/rules response must be array, got {body}"))?
        .clone())
}

#[tokio::test]
async fn list_filter_rules_reports_never_fired_rule_present_with_zero_counts() -> anyhow::Result<()>
{
    let upstream = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset — skipping \
             list_filter_rules_reports_never_fired_rule_present_with_zero_counts"
        );
        return Ok(());
    };
    let client = reqwest::Client::new();
    let run_marker = uuid::Uuid::new_v4();

    let never_fires_id = create_rule(
        &client,
        &proxy.inspect_url,
        &format!("never-fires-{run_marker}"),
    )
    .await?;
    let fires_id = create_rule(&client, &proxy.inspect_url, &format!("fires-{run_marker}")).await?;

    let session_uuid = uuid::Uuid::new_v4().to_string();
    let sp_id =
        insert_request_with_system_prompt_at(&proxy.pool, &session_uuid, 900_001, Utc::now())
            .await?;
    insert_firing(&proxy.pool, sp_id, fires_id).await?;

    let rows = fetch_filter_rules(&client, &proxy.inspect_url).await?;

    let never_fires_key = never_fires_id.to_string();
    let fires_key = fires_id.to_string();

    let never_fires_row = rows
        .iter()
        .find(|r| r["id"] == never_fires_key)
        .unwrap_or_else(|| {
            panic!(
                "never-fired rule (id {never_fires_id}) missing from response — must be \
                 present with zero counts, not absent: {rows:?}"
            )
        });
    assert_eq!(never_fires_row["fires_24h"], 0);
    assert_eq!(never_fires_row["fires_7d"], 0);
    assert_eq!(never_fires_row["last_fired_at"], Value::Null);

    let fires_row = rows
        .iter()
        .find(|r| r["id"] == fires_key)
        .unwrap_or_else(|| panic!("fired rule (id {fires_id}) missing from response: {rows:?}"));
    assert_eq!(fires_row["fires_24h"], 1);
    assert_eq!(fires_row["fires_7d"], 1);
    assert!(
        fires_row["last_fired_at"].is_string(),
        "expected non-null last_fired_at, got {fires_row:?}"
    );

    proxy.shutdown.cancel();
    proxy.pool.close().await;
    proxy.test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn list_filter_rules_separates_24h_and_7d_windows() -> anyhow::Result<()> {
    let upstream = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset — skipping list_filter_rules_separates_24h_and_7d_windows"
        );
        return Ok(());
    };
    let client = reqwest::Client::new();
    let run_marker = uuid::Uuid::new_v4();

    let rule_id = create_rule(
        &client,
        &proxy.inspect_url,
        &format!("stale-fire-{run_marker}"),
    )
    .await?;

    let session_uuid = uuid::Uuid::new_v4().to_string();
    let three_days_ago = Utc::now() - chrono::Duration::days(3);
    let sp_id =
        insert_request_with_system_prompt_at(&proxy.pool, &session_uuid, 900_002, three_days_ago)
            .await?;
    insert_firing(&proxy.pool, sp_id, rule_id).await?;

    let rows = fetch_filter_rules(&client, &proxy.inspect_url).await?;
    let rule_key = rule_id.to_string();
    let row = rows
        .iter()
        .find(|r| r["id"] == rule_key)
        .unwrap_or_else(|| panic!("rule (id {rule_id}) missing from response: {rows:?}"));

    assert_eq!(
        row["fires_24h"], 0,
        "fire is 3 days old — must not count toward the 24h window: {row:?}"
    );
    assert_eq!(
        row["fires_7d"], 1,
        "fire is within 7 days — must count toward the 7d window: {row:?}"
    );
    assert!(
        row["last_fired_at"].is_string(),
        "expected non-null last_fired_at (fire is within the 7d join window), got {row:?}"
    );

    proxy.shutdown.cancel();
    proxy.pool.close().await;
    proxy.test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn patch_filter_rule_reports_real_firing_counts_not_reset_to_zero() -> anyhow::Result<()> {
    let upstream = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset — skipping \
             patch_filter_rule_reports_real_firing_counts_not_reset_to_zero"
        );
        return Ok(());
    };
    let client = reqwest::Client::new();
    let run_marker = uuid::Uuid::new_v4();

    let rule_id = create_rule(
        &client,
        &proxy.inspect_url,
        &format!("patch-preserves-{run_marker}"),
    )
    .await?;

    let session_uuid = uuid::Uuid::new_v4().to_string();
    let sp_id =
        insert_request_with_system_prompt_at(&proxy.pool, &session_uuid, 900_003, Utc::now())
            .await?;
    insert_firing(&proxy.pool, sp_id, rule_id).await?;

    let patch_resp = client
        .patch(format!("{}/api/filter/rules/{rule_id}", proxy.inspect_url))
        .json(&json!({"enabled": false}))
        .send()
        .await?;
    let status = patch_resp.status();
    let body: Value = patch_resp.json().await?;
    anyhow::ensure!(status == 200, "PATCH expected 200, got {status}: {body}");

    assert_eq!(
        body["rule"]["fires_24h"], 1,
        "patch response must report the real firing count, not reset to 0: {body}"
    );
    assert_eq!(body["rule"]["fires_7d"], 1);
    assert!(
        body["rule"]["last_fired_at"].is_string(),
        "patch response must keep the real last_fired_at, not null it out: {body}"
    );

    proxy.shutdown.cancel();
    proxy.pool.close().await;
    proxy.test_db.cleanup().await?;
    Ok(())
}
