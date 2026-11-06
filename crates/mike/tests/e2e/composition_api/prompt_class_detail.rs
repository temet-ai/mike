use crate::db_helpers;
use crate::upstream;

use serde_json::{Value, json};

use super::support::{fire_request_and_get_session_and_request_ids, send_canonical_request};

async fn seed_session_prompt_row(
    pool: &sqlx::PgPool,
    session_id: i64,
    request_id: i64,
    lane: &str,
    subagent_persona: Option<&str>,
    content_sha256_marker: &str,
) -> anyhow::Result<()> {
    let blocks = json!([
        {"type": "text", "text": "billing-header"},
        {"type": "text", "text": "identity preamble"},
        {"type": "text", "text": "role body"},
    ]);
    sqlx::query(
        "INSERT INTO system_prompts.session_prompts
            (session_id, request_id, lane, subagent_persona, blocks, content_sha256, altered)
         VALUES ($1, $2, $3, $4, $5, $6, false)",
    )
    .bind(session_id)
    .bind(request_id)
    .bind(lane)
    .bind(subagent_persona)
    .bind(&blocks)
    .bind(content_sha256_marker)
    .execute(pool)
    .await?;
    Ok(())
}

fn manual_url_encode(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            ' ' => "%20".to_string(),
            '+' => "%2B".to_string(),
            other => other.to_string(),
        })
        .collect()
}

#[tokio::test]
async fn list_prompt_classes_returns_main_lane_row_with_block_count() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let run_marker = uuid::Uuid::new_v4().to_string();
    let session_uuid = uuid::Uuid::new_v4().to_string();
    send_canonical_request(&client, &proxy_url, &session_uuid, "role body")
        .await?
        .error_for_status()?;

    let resp = client
        .get(format!("{inspect_url}/api/prompts/classes"))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let classes: Value = resp.json().await?;
    let rows = classes
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("expected an array, got {classes}"))?;
    let main_row = rows
        .iter()
        .find(|r| r["lane"].as_str() == Some("main"))
        .ok_or_else(|| {
            anyhow::anyhow!("no lane=main row in catalogue: {rows:?}, marker {run_marker}")
        })?;
    assert!(
        main_row["block_count"].as_u64().unwrap_or(0) > 0,
        "main row must report a nonzero block_count: {main_row}"
    );
    assert!(
        main_row["observation_count"].as_i64().unwrap_or(0) >= 1,
        "main row must have counted the fired request: {main_row}"
    );
    assert!(
        main_row.get("sample_blocks").is_none(),
        "catalogue rows must stay lean (block_count only) per catalogue-query.md; \
         sample_blocks belongs to the detail route: {main_row}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn get_prompt_class_detail_main_includes_sample_blocks_and_recent_observations()
-> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let session_uuid = uuid::Uuid::new_v4().to_string();
    send_canonical_request(&client, &proxy_url, &session_uuid, "role body")
        .await?
        .error_for_status()?;
    let request_id = sqlx::query_scalar::<_, i64>(
        "SELECT r.id FROM requests.requests r
           JOIN sessions.sessions s ON s.id = r.session_id
          WHERE s.session_uuid = $1::uuid
          ORDER BY r.id DESC LIMIT 1",
    )
    .bind(&session_uuid)
    .fetch_one(&pool)
    .await?;

    let resp = client
        .get(format!("{inspect_url}/api/prompts/classes/main"))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let detail: Value = resp.json().await?;
    assert_eq!(detail["lane"].as_str(), Some("main"));
    assert!(detail["subagent_persona"].is_null());
    let sample_blocks = detail["sample_blocks"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("sample_blocks must be a non-null array: {detail}"))?;
    assert_eq!(
        sample_blocks.len(),
        3,
        "sample_blocks must be the full, unsliced 3-block array: {detail}"
    );
    let observations = detail["recent_observations"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("recent_observations must be an array: {detail}"))?;
    assert!(
        observations
            .iter()
            .any(|o| o["request_id"].as_str() == Some(&request_id.to_string())),
        "recent_observations must include the fired request {request_id}: {observations:?}"
    );
    assert!(
        detail["observations_total_count"].as_i64().unwrap_or(0) >= 1,
        "observations_total_count must reflect the fired request: {detail}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn get_prompt_class_detail_subagent_unresolved_segment_shadows_literal_persona_string()
-> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let run_marker = uuid::Uuid::new_v4().simple().to_string();
    let null_marker = format!("null-persona-marker-{run_marker}");
    let literal_marker = format!("literal-unresolved-marker-{run_marker}");

    let null_session = uuid::Uuid::new_v4().to_string();
    let (null_sid, null_rid) = fire_request_and_get_session_and_request_ids(
        &client,
        &proxy_url,
        &pool,
        &null_session,
        &format!("subagent-null-persona-{run_marker}"),
    )
    .await?;
    seed_session_prompt_row(&pool, null_sid, null_rid, "subagent", None, &null_marker).await?;

    let literal_session = uuid::Uuid::new_v4().to_string();
    let (literal_sid, literal_rid) = fire_request_and_get_session_and_request_ids(
        &client,
        &proxy_url,
        &pool,
        &literal_session,
        &format!("subagent-literal-unresolved-{run_marker}"),
    )
    .await?;
    seed_session_prompt_row(
        &pool,
        literal_sid,
        literal_rid,
        "subagent",
        Some("unresolved"),
        &literal_marker,
    )
    .await?;

    let resp = client
        .get(format!(
            "{inspect_url}/api/prompts/classes/subagent/unresolved"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let detail: Value = resp.json().await?;
    assert!(detail["subagent_persona"].is_null());
    let markers: Vec<&str> = detail["recent_observations"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("recent_observations must be an array: {detail}"))?
        .iter()
        .filter_map(|o| o["content_sha256"].as_str())
        .collect();
    assert!(
        markers.contains(&null_marker.as_str()),
        "the null-persona row must appear: {markers:?}"
    );
    assert!(
        !markers.contains(&literal_marker.as_str()),
        "the literal 'unresolved'-persona row must NOT appear under the \
         `unresolved` segment — it belongs to its own, currently \
         unreachable-via-URL class: {markers:?}"
    );
    assert!(
        detail["observations_total_count"].as_i64().unwrap_or(0) >= 1,
        "observations_total_count must reflect the null-persona row's traffic: {detail}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn get_prompt_class_detail_subagent_persona_with_plus_survives_as_literal_plus()
-> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let run_marker = uuid::Uuid::new_v4().simple().to_string();
    let persona = format!("Postgres + Rust Database Specialist {run_marker}");
    let session_uuid = uuid::Uuid::new_v4().to_string();
    let (session_id, request_id) = fire_request_and_get_session_and_request_ids(
        &client,
        &proxy_url,
        &pool,
        &session_uuid,
        &format!("subagent-plus-persona-{run_marker}"),
    )
    .await?;
    seed_session_prompt_row(
        &pool,
        session_id,
        request_id,
        "subagent",
        Some(&persona),
        &format!("plus-marker-{run_marker}"),
    )
    .await?;

    let encoded = manual_url_encode(&persona);
    let resp = client
        .get(format!(
            "{inspect_url}/api/prompts/classes/subagent/{encoded}"
        ))
        .send()
        .await?;
    assert_eq!(
        resp.status(),
        200,
        "GET on the encoded persona path must resolve, got {}",
        resp.status()
    );
    let detail: Value = resp.json().await?;
    assert_eq!(
        detail["subagent_persona"].as_str(),
        Some(persona.as_str()),
        "the '+' in the persona must decode to a literal plus, not a space: {detail}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn get_prompt_class_detail_subagent_unknown_persona_returns_404() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let run_marker = uuid::Uuid::new_v4().simple().to_string();
    let resp = client
        .get(format!(
            "{inspect_url}/api/prompts/classes/subagent/nonexistent-persona-{run_marker}"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), 404);

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
