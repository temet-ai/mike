use crate::db_helpers;
use crate::upstream;

use serde_json::{Value, json};

use super::support::{
    create_via_api, find_upstream_body_for_session, force_mode, prompt_sha256_for,
    send_canonical_request,
};

#[tokio::test]
async fn patch_flips_mode_template_and_next_request_applies() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(test_db_factory) = db_helpers::TestDb::bootstrap().await? else {
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;
    let (proxy_url, inspect_url, shutdown) =
        db_helpers::start_proxy_with_db_at_composed(&upstream.uri(), &test_db.database_url).await?;
    let client = reqwest::Client::new();

    let block2_original = "patch-template-original-body";
    let prompt_sha256 = prompt_sha256_for(block2_original);

    let envelope = create_via_api(
        &client,
        &inspect_url,
        &prompt_sha256,
        "patch-template",
        "REWRITTEN-BODY",
    )
    .await?;
    let composition_id = envelope["composition"]["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("composition.id absent: {envelope}"))?
        .to_string();

    force_mode(&pool, &prompt_sha256, "template").await?;

    let patch_resp = client
        .patch(format!(
            "{inspect_url}/api/prompts/compositions/{composition_id}"
        ))
        .json(&json!({"label": "patch-template-v2"}))
        .send()
        .await?;
    assert_eq!(patch_resp.status(), 200);
    let patched: Value = patch_resp.json().await?;
    assert_eq!(patched["composition"]["label"], "patch-template-v2");
    assert_eq!(
        patched["composition"]["version"].as_i64(),
        Some(2),
        "PATCH must bump version"
    );

    let session_id = "11111111-2222-3333-4444-555555555555";
    let resp = send_canonical_request(&client, &proxy_url, session_id, block2_original).await?;
    assert_eq!(resp.status(), 200);

    let upstream_body = find_upstream_body_for_session(&upstream, session_id).await?;
    let upstream_block2 = upstream_body["system"][2]["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("upstream block 2 text missing: {upstream_body}"))?;
    assert_eq!(
        upstream_block2, "REWRITTEN-BODY",
        "composer in template mode must rewrite block 2 upstream; saw {upstream_block2:?}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn delete_falls_back_to_classified_passthrough() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(test_db_factory) = db_helpers::TestDb::bootstrap().await? else {
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;
    let (proxy_url, inspect_url, shutdown) =
        db_helpers::start_proxy_with_db_at_composed(&upstream.uri(), &test_db.database_url).await?;
    let client = reqwest::Client::new();

    let block2_original = "delete-fallback-body";
    let prompt_sha256 = prompt_sha256_for(block2_original);

    let envelope = create_via_api(
        &client,
        &inspect_url,
        &prompt_sha256,
        "delete-fallback",
        "WOULD-BE-REWRITTEN",
    )
    .await?;
    let composition_id = envelope["composition"]["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("composition.id absent: {envelope}"))?
        .to_string();
    force_mode(&pool, &prompt_sha256, "template").await?;
    client
        .patch(format!(
            "{inspect_url}/api/prompts/compositions/{composition_id}"
        ))
        .json(&json!({"label": "delete-fallback-armed"}))
        .send()
        .await?;

    let del = client
        .delete(format!(
            "{inspect_url}/api/prompts/compositions/{composition_id}"
        ))
        .send()
        .await?;
    assert_eq!(del.status(), 200, "DELETE must 200 on success");
    let del_body: Value = del.json().await?;
    assert_eq!(
        del_body["warnings"],
        Value::Array(vec![]),
        "expected empty warnings on clean delete"
    );

    let after = client
        .get(format!(
            "{inspect_url}/api/prompts/compositions/{composition_id}"
        ))
        .send()
        .await?;
    assert_eq!(after.status(), 404);

    let session_id = "99999999-aaaa-bbbb-cccc-dddddddddddd";
    let resp = send_canonical_request(&client, &proxy_url, session_id, block2_original).await?;
    assert_eq!(resp.status(), 200);

    let upstream_body = find_upstream_body_for_session(&upstream, session_id).await?;
    assert_eq!(
        upstream_body["system"][2]["text"].as_str(),
        Some(block2_original),
        "after DELETE the composer must passthrough the original bytes"
    );

    let summaries: Vec<Value> = client
        .get(format!("{inspect_url}/api/requests"))
        .send()
        .await?
        .json()
        .await?;
    let request_id = summaries
        .iter()
        .find(|s| s.get("session_id").and_then(|v| v.as_str()) == Some(session_id))
        .and_then(|s| s.get("id"))
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("request not surfaced"))?;
    let detail: Value = client
        .get(format!("{inspect_url}/api/requests/{request_id}"))
        .send()
        .await?
        .json()
        .await?;
    let actual_prompt_sha256 = detail["system_blocks"]["prompt_sha256"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("prompt_sha256 missing on request detail: {detail}"))?;
    assert_eq!(
        actual_prompt_sha256, prompt_sha256,
        "DELETE removes the composition but classification still tags the request"
    );
    assert_eq!(detail["composition_applied"], Value::Bool(false));

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn patch_updates_cache_and_new_session_observes_change() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(test_db_factory) = db_helpers::TestDb::bootstrap().await? else {
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;
    let (proxy_url, inspect_url, shutdown) =
        db_helpers::start_proxy_with_db_at_composed(&upstream.uri(), &test_db.database_url).await?;
    let client = reqwest::Client::new();

    let block2_original = "cache-invalidation-body";
    let prompt_sha256 = prompt_sha256_for(block2_original);

    let envelope = create_via_api(
        &client,
        &inspect_url,
        &prompt_sha256,
        "cache-inval-v1",
        "v1-text",
    )
    .await?;
    let composition_id = envelope["composition"]["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("composition.id absent: {envelope}"))?
        .to_string();
    force_mode(&pool, &prompt_sha256, "template").await?;
    client
        .patch(format!(
            "{inspect_url}/api/prompts/compositions/{composition_id}"
        ))
        .json(&json!({"label": "cache-inval-v1-armed"}))
        .send()
        .await?;

    let session_id_a = "aaaaaaaa-1111-2222-3333-444444444444";
    let resp_a = send_canonical_request(&client, &proxy_url, session_id_a, block2_original).await?;
    assert_eq!(resp_a.status(), 200);

    let patch_resp = client
        .patch(format!(
            "{inspect_url}/api/prompts/compositions/{composition_id}"
        ))
        .json(&json!({
            "blocks": [
                {"text": "billing-header"},
                {"text": "identity preamble"},
                {"text": "v2-text"},
            ]
        }))
        .send()
        .await?;
    assert_eq!(patch_resp.status(), 200);
    let patched: Value = patch_resp.json().await?;
    assert_eq!(patched["composition"]["version"].as_i64(), Some(3));
    let new_blocks = patched["composition"]["blocks"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("blocks absent: {patched}"))?;
    assert_eq!(new_blocks[2]["text"], "v2-text");

    let session_id_b = "bbbbbbbb-1111-2222-3333-444444444444";
    let resp_b = send_canonical_request(&client, &proxy_url, session_id_b, block2_original).await?;
    assert_eq!(resp_b.status(), 200);

    let body_b = find_upstream_body_for_session(&upstream, session_id_b).await?;
    assert_eq!(
        body_b["system"][2]["text"].as_str(),
        Some("v2-text"),
        "cache must observe the PATCH that swapped block 2 text"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn patch_mode_template_round_trips_via_api() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(test_db_factory) = db_helpers::TestDb::bootstrap().await? else {
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;
    let (proxy_url, inspect_url, shutdown) =
        db_helpers::start_proxy_with_db_at_composed(&upstream.uri(), &test_db.database_url).await?;
    let client = reqwest::Client::new();

    let block2_original = "patch-mode-via-api-original";
    let prompt_sha256 = prompt_sha256_for(block2_original);

    let envelope = create_via_api(
        &client,
        &inspect_url,
        &prompt_sha256,
        "patch-mode-via-api",
        "REWRITTEN-MODE-VIA-API",
    )
    .await?;
    let composition_id = envelope["composition"]["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("composition.id absent"))?
        .to_string();
    assert_eq!(envelope["composition"]["mode"].as_str(), Some("observe"));

    let patched = client
        .patch(format!(
            "{inspect_url}/api/prompts/compositions/{composition_id}"
        ))
        .json(&json!({"mode": "template"}))
        .send()
        .await?;
    assert_eq!(patched.status(), 200);
    let body: Value = patched.json().await?;
    assert_eq!(
        body["composition"]["mode"].as_str(),
        Some("template"),
        "PATCH must round-trip mode on the wire shape"
    );
    assert_eq!(body["composition"]["version"].as_i64(), Some(2));

    let session_id = "11111111-cafe-cafe-cafe-cafecafecafe";
    let resp = send_canonical_request(&client, &proxy_url, session_id, block2_original).await?;
    assert_eq!(resp.status(), 200);
    let upstream_body = find_upstream_body_for_session(&upstream, session_id).await?;
    assert_eq!(
        upstream_body["system"][2]["text"].as_str(),
        Some("REWRITTEN-MODE-VIA-API"),
        "PATCH {{mode: template}} must invalidate the cache so the very next request rewrites"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
