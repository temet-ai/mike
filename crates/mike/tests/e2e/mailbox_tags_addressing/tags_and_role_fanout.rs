use crate::db_helpers;

use serde_json::{Value, json};
use sqlx::PgPool;
use wiremock::MockServer;

use super::support::{ensure_session, seed_live_registration, seed_role};

struct TagsAddressingFixture {
    coordinator: String,
    worker_a: String,
    worker_b: String,
    other_role: String,
}

#[tokio::test]
async fn tags_seed_filter_and_role_fanout() -> anyhow::Result<()> {
    let upstream = MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping mailbox_tags_addressing");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let fixture = TagsAddressingFixture {
        coordinator: "dd000000-0000-4000-e001-000000000001".to_string(),
        worker_a: "dd000000-0000-4000-e002-000000000001".to_string(),
        worker_b: "dd000000-0000-4000-e003-000000000001".to_string(),
        other_role: "dd000000-0000-4000-e004-000000000001".to_string(),
    };
    let project_path = "mailbox-tags-addressing-test";

    let (coordinator_id, project_id) =
        ensure_session(&pool, &fixture.coordinator, project_path).await?;
    let (worker_a_id, _) = ensure_session(&pool, &fixture.worker_a, project_path).await?;
    let (worker_b_id, _) = ensure_session(&pool, &fixture.worker_b, project_path).await?;
    let (other_id, _) = ensure_session(&pool, &fixture.other_role, project_path).await?;

    seed_live_registration(&pool, coordinator_id, project_id).await?;
    seed_live_registration(&pool, worker_a_id, project_id).await?;
    seed_live_registration(&pool, worker_b_id, project_id).await?;
    seed_live_registration(&pool, other_id, project_id).await?;
    seed_role(&pool, worker_a_id, "rust-developer").await?;
    seed_role(&pool, worker_b_id, "rust-developer").await?;
    seed_role(&pool, other_id, "frontend-developer").await?;

    let client = reqwest::Client::new();

    assert_tag_codes_present(&client, &inspect_url).await?;
    assert_tag_intern_and_filter(&client, &inspect_url, &fixture).await?;
    assert_role_fanout(&client, &inspect_url, &pool, &fixture).await?;
    assert_role_fanout_rejects_unmatched_role_and_missing_subject(
        &client,
        &inspect_url,
        &fixture.coordinator,
    )
    .await?;
    assert_uuid_prefix_send_keeps_legacy_shape(&client, &inspect_url, &fixture).await?;

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

async fn assert_tag_codes_present(
    client: &reqwest::Client,
    inspect_url: &str,
) -> anyhow::Result<()> {
    let tags_resp = client.get(format!("{inspect_url}/api/tags")).send().await?;
    anyhow::ensure!(
        tags_resp.status() == 200,
        "GET /api/tags returned {}",
        tags_resp.status()
    );
    let tags_body: Value = tags_resp.json().await?;
    let tag_codes: Vec<&str> = tags_body["tags"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("tags field not an array: {tags_body}"))?
        .iter()
        .filter_map(|t| t["code"].as_str())
        .collect();
    for expected in [
        "handoff", "blocked", "review", "merge", "redeploy", "heads-up", "question", "decision",
    ] {
        anyhow::ensure!(
            tag_codes.contains(&expected),
            "seeded tag `{expected}` missing from GET /api/tags: {tag_codes:?}"
        );
    }
    Ok(())
}

async fn assert_tag_intern_and_filter(
    client: &reqwest::Client,
    inspect_url: &str,
    fixture: &TagsAddressingFixture,
) -> anyhow::Result<()> {
    let unknown_code = "unit-test-tag";
    let tagged_send = client
        .post(format!(
            "{inspect_url}/api/sessions/{}/message",
            fixture.worker_a
        ))
        .json(&json!({
            "message": "please review the migration",
            "from_session_id": fixture.coordinator,
            "subject": "migration review",
            "tags": ["review", unknown_code],
        }))
        .send()
        .await?;
    anyhow::ensure!(
        tagged_send.status() == 201,
        "tagged send returned {}",
        tagged_send.status()
    );

    let filtered_resp = client
        .get(format!("{inspect_url}/api/threads?tag=review"))
        .send()
        .await?;
    anyhow::ensure!(
        filtered_resp.status() == 200,
        "GET /api/threads?tag=review returned {}",
        filtered_resp.status()
    );
    let filtered: Value = filtered_resp.json().await?;
    let filtered_threads = filtered["threads"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("threads not an array: {filtered}"))?;
    anyhow::ensure!(
        filtered_threads
            .iter()
            .any(|t| t["subject"].as_str() == Some("migration review")),
        "?tag=review must return the migration-review thread: {filtered}"
    );
    let tagged_thread = filtered_threads
        .iter()
        .find(|t| t["subject"].as_str() == Some("migration review"))
        .ok_or_else(|| anyhow::anyhow!("migration-review thread not found: {filtered}"))?;
    let thread_tags: Vec<&str> = tagged_thread["tags"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("tags not an array: {tagged_thread}"))?
        .iter()
        .filter_map(|t| t.as_str())
        .collect();
    anyhow::ensure!(
        thread_tags.contains(&"review") && thread_tags.contains(&unknown_code),
        "an unknown tag code must be interned and attached alongside the known one: {thread_tags:?}"
    );

    let unfiltered_resp = client
        .get(format!("{inspect_url}/api/threads?tag=blocked"))
        .send()
        .await?;
    let unfiltered: Value = unfiltered_resp.json().await?;
    anyhow::ensure!(
        !unfiltered["threads"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("threads not an array: {unfiltered}"))?
            .iter()
            .any(|t| t["subject"].as_str() == Some("migration review")),
        "?tag=blocked must NOT return a thread tagged only `review`/`{unknown_code}`: {unfiltered}"
    );
    Ok(())
}

async fn assert_role_fanout(
    client: &reqwest::Client,
    inspect_url: &str,
    pool: &PgPool,
    fixture: &TagsAddressingFixture,
) -> anyhow::Result<()> {
    let fanout_resp = client
        .post(format!(
            "{inspect_url}/api/sessions/role:rust-developer/message"
        ))
        .json(&json!({
            "message": "heads up: deploying in 10 minutes",
            "from_session_id": fixture.coordinator,
            "subject": "deploy heads-up",
        }))
        .send()
        .await?;
    anyhow::ensure!(
        fanout_resp.status() == 201,
        "role: fan-out returned {}: {}",
        fanout_resp.status(),
        fanout_resp.text().await.unwrap_or_default()
    );
    let fanout: Value = fanout_resp.json().await?;
    let mut fanout_keys: Vec<&str> = fanout
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("fan-out response not an object: {fanout}"))?
        .keys()
        .map(String::as_str)
        .collect();
    fanout_keys.sort_unstable();
    anyhow::ensure!(
        fanout_keys == ["deliveries", "subject", "thread_uuid"],
        "role: fan-out response shape must be {{thread_uuid, subject, deliveries}}: got {fanout_keys:?}"
    );
    anyhow::ensure!(
        fanout["subject"].as_str() == Some("deploy heads-up"),
        "fan-out response subject must echo the request: {fanout}"
    );
    let deliveries = fanout["deliveries"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("deliveries not an array: {fanout}"))?;
    anyhow::ensure!(
        deliveries.len() == 2,
        "role:rust-developer must fan out to exactly worker_a and worker_b, not \
         other_role (frontend-developer): got {} deliveries: {fanout}",
        deliveries.len()
    );
    let delivery_recipients: Vec<&str> = deliveries
        .iter()
        .filter_map(|d| d["to_session_uuid"].as_str())
        .collect();
    anyhow::ensure!(
        delivery_recipients.contains(&fixture.worker_a.as_str())
            && delivery_recipients.contains(&fixture.worker_b.as_str()),
        "fan-out deliveries must address worker_a and worker_b: {delivery_recipients:?}"
    );
    anyhow::ensure!(
        !delivery_recipients.contains(&fixture.other_role.as_str()),
        "fan-out must NOT address a session with a different declared role: {delivery_recipients:?}"
    );
    let fanout_thread_uuid = fanout["thread_uuid"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing thread_uuid: {fanout}"))?;
    let (row_count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM mailbox.messages m
          JOIN mailbox.threads t ON t.id = m.thread_id
         WHERE t.thread_uuid = $1::text::uuid",
    )
    .bind(fanout_thread_uuid)
    .fetch_one(pool)
    .await?;
    anyhow::ensure!(
        row_count == 2,
        "the fan-out must write exactly N mailbox.messages rows sharing one thread_id, \
         never a single row with a recipient list: got {row_count}"
    );
    Ok(())
}

async fn assert_role_fanout_rejects_unmatched_role_and_missing_subject(
    client: &reqwest::Client,
    inspect_url: &str,
    coordinator: &str,
) -> anyhow::Result<()> {
    let no_match_resp = client
        .post(format!(
            "{inspect_url}/api/sessions/role:nonexistent-role/message"
        ))
        .json(&json!({
            "message": "nobody home",
            "from_session_id": coordinator,
            "subject": "nobody home",
        }))
        .send()
        .await?;
    anyhow::ensure!(
        no_match_resp.status() == 404,
        "an unmatched role must 404, got {}",
        no_match_resp.status()
    );

    let no_subject_resp = client
        .post(format!(
            "{inspect_url}/api/sessions/role:rust-developer/message"
        ))
        .json(&json!({
            "message": "missing subject",
            "from_session_id": coordinator,
        }))
        .send()
        .await?;
    anyhow::ensure!(
        no_subject_resp.status() == 400,
        "role: fan-out with no subject/thread_id must 400, got {}",
        no_subject_resp.status()
    );
    Ok(())
}

async fn assert_uuid_prefix_send_keeps_legacy_shape(
    client: &reqwest::Client,
    inspect_url: &str,
    fixture: &TagsAddressingFixture,
) -> anyhow::Result<()> {
    let prefix = &fixture.worker_a[..23];
    let prefix_resp = client
        .post(format!("{inspect_url}/api/sessions/{prefix}/message"))
        .json(&json!({ "message": "prefix-addressed", "from_session_id": fixture.coordinator }))
        .send()
        .await?;
    anyhow::ensure!(
        prefix_resp.status() == 201,
        "uuid-prefix send returned {}",
        prefix_resp.status()
    );
    let prefix_body: Value = prefix_resp.json().await?;
    let mut prefix_keys: Vec<&str> = prefix_body
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("prefix response not an object: {prefix_body}"))?
        .keys()
        .map(String::as_str)
        .collect();
    prefix_keys.sort_unstable();
    let mut expected_single_keys = vec![
        "message_uuid",
        "to_session_uuid",
        "from_session_uuid",
        "queued_at",
        "recipient_liveness",
        "recipient_last_request_at",
        "recipient_last_eligible_turn_at",
        "delivery_state",
    ];
    expected_single_keys.sort_unstable();
    anyhow::ensure!(
        prefix_keys == expected_single_keys,
        "a uuid-prefix send must keep the exact legacy single-recipient shape: got \
         {prefix_keys:?}, expected {expected_single_keys:?}"
    );
    anyhow::ensure!(
        prefix_body["to_session_uuid"].as_str() == Some(fixture.worker_a.as_str()),
        "prefix must resolve to worker_a: {prefix_body}"
    );
    Ok(())
}
