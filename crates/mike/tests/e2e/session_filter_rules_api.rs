use crate::db_helpers;

use serde_json::Value;
use sqlx::PgPool;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn mock_upstream() -> MockServer {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_session_filter_rules_test",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "ack"}],
            "model": "claude-sonnet-4-20250514",
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 1, "output_tokens": 1}
        })))
        .mount(&mock_server)
        .await;
    mock_server
}

async fn insert_request_with_system_prompt(
    pool: &PgPool,
    session_uuid: &str,
    request_id: i64,
) -> anyhow::Result<i64> {
    let project_id: i64 = sqlx::query_scalar(
        "WITH ins AS (
             INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
             SELECT $1, $1, NOW(), NOW()
             WHERE NOT EXISTS (
                 SELECT 1 FROM projects.projects WHERE project_path = $1
             )
             RETURNING id
         )
         SELECT id FROM ins
         UNION ALL
         SELECT id FROM projects.projects WHERE project_path = $1
         LIMIT 1",
    )
    .bind("session-filter-rules-test-project")
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
            clock_timestamp()
         )",
    )
    .bind(request_id)
    .bind(session_uuid)
    .bind(project_id)
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

async fn insert_rule(
    pool: &PgPool,
    kind_code: &str,
    value: &str,
    trigger_token: Option<&str>,
    enabled: bool,
) -> anyhow::Result<i64> {
    let action_code = if kind_code == "lazy_section" {
        "replace"
    } else {
        "drop"
    };
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO rules.rules
            (uuid, kind_id, value, action_id, trigger_token, enabled, source_id, sort_order)
         VALUES (
            gen_random_uuid(),
            (SELECT id FROM rules.kinds WHERE code = $1),
            $2,
            (SELECT id FROM rules.actions WHERE code = $3),
            $4,
            $5,
            (SELECT id FROM rules.sources WHERE code = 'ui'),
            0
         )
         RETURNING id",
    )
    .bind(kind_code)
    .bind(value)
    .bind(action_code)
    .bind(trigger_token)
    .bind(enabled)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

async fn insert_firing(
    pool: &PgPool,
    system_prompt_id: i64,
    rule_id: i64,
    ord: i32,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO requests.system_prompt_firing_rules
            (request_system_prompt_id, filter_rule_id, ord)
         VALUES ($1, $2, $3)",
    )
    .bind(system_prompt_id)
    .bind(rule_id)
    .bind(ord)
    .execute(pool)
    .await?;
    Ok(())
}

async fn fetch_session_filter_rules(
    client: &reqwest::Client,
    inspect_url: &str,
    session_uuid: &str,
) -> anyhow::Result<Vec<Value>> {
    let resp = client
        .get(format!(
            "{inspect_url}/api/sessions/{session_uuid}/filter-rules"
        ))
        .send()
        .await?;
    let status = resp.status();
    let body: Value = resp.json().await?;
    anyhow::ensure!(
        status == 200,
        "GET .../filter-rules expected 200, got {status}: {body}"
    );
    Ok(body
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("filter-rules response must be array, got {body}"))?
        .clone())
}

#[tokio::test]
async fn get_session_filter_rules_returns_empty_when_nothing_fired() -> anyhow::Result<()> {
    let upstream = mock_upstream().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset — skipping \
             get_session_filter_rules_returns_empty_when_nothing_fired"
        );
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let session_uuid = "22222222-aaaa-bbbb-cccc-000000000001";
    insert_request_with_system_prompt(&pool, session_uuid, 101).await?;

    let rows = fetch_session_filter_rules(&client, &inspect_url, session_uuid).await?;
    assert!(
        rows.is_empty(),
        "expected no rows for a session with no firing-rule junction rows, got {rows:?}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn get_session_filter_rules_returns_fired_rules_with_turn_counts() -> anyhow::Result<()> {
    let upstream = mock_upstream().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset — skipping \
             get_session_filter_rules_returns_fired_rules_with_turn_counts"
        );
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let session_uuid = "22222222-aaaa-bbbb-cccc-000000000002";
    let sp1 = insert_request_with_system_prompt(&pool, session_uuid, 201).await?;
    let sp2 = insert_request_with_system_prompt(&pool, session_uuid, 202).await?;

    let rule_a = insert_rule(&pool, "text_substring", "rule-a-value", None, true).await?;
    insert_firing(&pool, sp1, rule_a, 0).await?;
    insert_firing(&pool, sp2, rule_a, 0).await?;

    let rule_b = insert_rule(
        &pool,
        "lazy_section",
        "## rule-b-heading",
        Some("!!RULE_B!!"),
        false,
    )
    .await?;
    insert_firing(&pool, sp1, rule_b, 1).await?;

    insert_rule(&pool, "text_substring", "rule-c-never-fires", None, true).await?;

    let rows = fetch_session_filter_rules(&client, &inspect_url, session_uuid).await?;
    assert_eq!(
        rows.len(),
        2,
        "expected exactly 2 fired rules (A and B), never-fired rule C omitted; got {rows:?}"
    );

    let row_a = rows
        .iter()
        .find(|r| r["rule_id"] == rule_a)
        .unwrap_or_else(|| panic!("rule A (id {rule_a}) missing from response: {rows:?}"));
    assert_eq!(row_a["kind"], "text_substring");
    assert_eq!(row_a["label"], "rule-a-value");
    assert_eq!(row_a["tag"], Value::Null);
    assert_eq!(row_a["enabled"], Value::Bool(true));
    assert_eq!(
        row_a["fired_turns"], 2,
        "rule A fired on 2 distinct requests"
    );

    let row_b = rows
        .iter()
        .find(|r| r["rule_id"] == rule_b)
        .unwrap_or_else(|| panic!("rule B (id {rule_b}) missing from response: {rows:?}"));
    assert_eq!(row_b["kind"], "lazy_section");
    assert_eq!(row_b["label"], "## rule-b-heading");
    assert_eq!(row_b["tag"], "!!RULE_B!!");
    assert_eq!(
        row_b["enabled"],
        Value::Bool(false),
        "rule B was disabled after firing; response must report CURRENT state"
    );
    assert_eq!(row_b["fired_turns"], 1);

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
