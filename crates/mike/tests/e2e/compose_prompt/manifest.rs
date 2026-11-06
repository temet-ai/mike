use crate::db_helpers;
use crate::upstream;

use serde_json::Value;
use sha2::{Digest, Sha256};

use super::support::{
    insert_composition, latest_request_id_for_session, prompt_sha256, send_canonical_request,
    wait_for_request_detail,
};

async fn wait_for_ledger_altered(
    pool: &sqlx::PgPool,
    session_uuid: uuid::Uuid,
) -> anyhow::Result<bool> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let row: Option<(bool,)> = sqlx::query_as(
            "SELECT sp.altered
             FROM system_prompts.session_prompts sp
             JOIN sessions.sessions s ON s.id = sp.session_id
             WHERE s.session_uuid = $1 AND sp.lane = 'main'",
        )
        .bind(session_uuid)
        .fetch_optional(pool)
        .await?;
        if let Some((altered,)) = row
            && altered
        {
            return Ok(true);
        }
        if std::time::Instant::now() >= deadline {
            return Ok(row.map(|(altered,)| altered).unwrap_or(false));
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn templated_composition_stamps_ledger_altered_true() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose").await;
    let Some(test_db_factory) = db_helpers::TestDb::bootstrap().await? else {
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let block2_in = "ledger-altered-stamp-role-body";
    let new_block2 = "REWRITTEN-FOR-LEDGER-ALTERED-TEST";
    insert_composition(
        &pool,
        &prompt_sha256(block2_in),
        "main-agent",
        new_block2,
        "template",
    )
    .await?;

    let (proxy_url, inspect_url, _shutdown) =
        db_helpers::start_proxy_with_db_at_composed(&upstream.uri(), &test_db.database_url).await?;
    let client = reqwest::Client::new();
    let session_id = "deadbeef-a1a1-4b2b-8c3c-000000000042";
    let session_uuid = uuid::Uuid::parse_str(session_id)?;

    let resp = send_canonical_request(&client, &proxy_url, session_id, block2_in).await?;
    assert_eq!(resp.status(), 200);

    let request_id = latest_request_id_for_session(&client, &inspect_url, session_id).await?;
    let detail = wait_for_request_detail(&client, &inspect_url, request_id).await?;
    assert_eq!(
        detail.get("composition_applied"),
        Some(&Value::Bool(true)),
        "test setup: request must take the Templated arm for the ledger stamp to fire"
    );

    let altered = wait_for_ledger_altered(&pool, session_uuid).await?;
    assert!(
        altered,
        "system_prompts.session_prompts main-lane row for this session must have \
         altered=true after a real Templated request completes end-to-end; if this \
         is false, the durable alter-decision stamp raced (or missed) the ledger \
         row's own creation"
    );

    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

async fn fetch_manifest_row(
    pool: &sqlx::PgPool,
    session_id: &str,
) -> anyhow::Result<(Option<Vec<u8>>, Option<Vec<u8>>, Option<Value>)> {
    let row: (Option<Vec<u8>>, Option<Vec<u8>>, Option<Value>) = sqlx::query_as(
        "SELECT r.request_body, r.filtered_request_body, r.transformation_manifest
         FROM requests.requests r
         JOIN sessions.sessions s ON s.id = r.session_id
         WHERE s.session_uuid = $1::uuid
         ORDER BY r.id DESC
         LIMIT 1",
    )
    .bind(session_id)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

async fn fetch_envelope_patch_len(
    pool: &sqlx::PgPool,
    request_id: i64,
) -> anyhow::Result<Option<i32>> {
    let row: Option<(Option<i32>,)> = sqlx::query_as(
        "SELECT octet_length(sent_patch_bytes)::int
         FROM requests.requests
         WHERE id = $1",
    )
    .bind(request_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|(v,)| v))
}

fn sha256_of(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn assert_manifest_chain_closes(manifest: &Value) -> anyhow::Result<()> {
    let original = manifest["original_sha256"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("manifest missing original_sha256"))?;
    let sent = manifest["sent_sha256"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("manifest missing sent_sha256"))?;
    let stages = manifest["stages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("manifest missing stages array"))?;
    let mut expected_input = original.to_string();
    for stage in stages {
        let input = stage["input_sha256"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("stage missing input_sha256: {stage:?}"))?;
        anyhow::ensure!(
            input == expected_input,
            "stage input_sha256 {input} does not chain to the previous output {expected_input}: {stage:?}"
        );
        expected_input = stage["output_sha256"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("stage missing output_sha256: {stage:?}"))?
            .to_string();
    }
    anyhow::ensure!(
        expected_input == sent,
        "final chain output {expected_input} does not equal manifest sent_sha256 {sent}"
    );
    Ok(())
}

async fn assert_envelope_patch_is_mutation_sized(
    pool: &sqlx::PgPool,
    request_id: u64,
    original_bytes: &[u8],
) -> anyhow::Result<()> {
    let envelope_patch_len = fetch_envelope_patch_len(pool, request_id as i64)
        .await?
        .ok_or_else(|| anyhow::anyhow!("requests.requests.sent_patch_bytes missing"))?;
    assert!(
        (envelope_patch_len as usize) < original_bytes.len() / 10,
        "envelope patch ({envelope_patch_len} bytes) must be on the order of the mutation, \
         not the {}-byte body",
        original_bytes.len()
    );
    Ok(())
}

async fn assert_sent_body_reconstructs_forwarded_bytes(
    client: &reqwest::Client,
    inspect_url: &str,
    request_id: u64,
    forwarded_bytes: &[u8],
    original_bytes: &[u8],
) -> anyhow::Result<()> {
    let sent_payload: Value = client
        .get(format!(
            "{inspect_url}/api/requests/{request_id}/body/request_sent"
        ))
        .send()
        .await?
        .json()
        .await?;
    let sent_text = sent_payload["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("request_sent body missing text"))?;
    assert_eq!(
        sent_text.as_bytes(),
        forwarded_bytes,
        "request_sent must reconstruct the exact bytes forwarded upstream"
    );
    assert_ne!(
        sent_text.as_bytes(),
        original_bytes,
        "sent body must differ from the client's original bytes when the composer templated"
    );
    Ok(())
}

fn assert_manifest_records_applied_composer_stage(
    manifest: &Value,
    original_bytes: &[u8],
    forwarded_bytes: &[u8],
) -> anyhow::Result<()> {
    assert_eq!(
        manifest["original_sha256"].as_str(),
        Some(sha256_of(original_bytes).as_str())
    );
    assert_eq!(
        manifest["sent_sha256"].as_str(),
        Some(sha256_of(forwarded_bytes).as_str())
    );
    assert_manifest_chain_closes(manifest)?;

    let stages = manifest["stages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("stages missing"))?;
    assert!(
        stages
            .iter()
            .any(|s| s["stage"] == "composer" && s["outcome"] == "applied"),
        "manifest must record an applied composer stage: {stages:?}"
    );
    Ok(())
}

#[tokio::test]
async fn templated_composition_stores_exact_sent_body_with_closed_manifest() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_manifest_templated").await;
    let Some(test_db_factory) = db_helpers::TestDb::bootstrap().await? else {
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let block2_in = "manifest-role-body-incoming";
    let new_block2 = "MANIFEST-REWRITTEN-ROLE-BODY";
    insert_composition(
        &pool,
        &prompt_sha256(block2_in),
        "main-agent",
        new_block2,
        "template",
    )
    .await?;

    let (proxy_url, inspect_url, _shutdown) =
        db_helpers::start_proxy_with_db_at_composed(&upstream.uri(), &test_db.database_url).await?;
    let client = reqwest::Client::new();
    let session_id = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeee1";

    let resp = send_canonical_request(&client, &proxy_url, session_id, block2_in).await?;
    assert_eq!(resp.status(), 200);

    let request_id = latest_request_id_for_session(&client, &inspect_url, session_id).await?;
    let detail = wait_for_request_detail(&client, &inspect_url, request_id).await?;
    assert_eq!(
        detail.get("composition_applied"),
        Some(&Value::Bool(true)),
        "test setup: request must take the Templated arm"
    );

    let received = upstream.received_requests().await;
    let received = received.ok_or_else(|| anyhow::anyhow!("no requests"))?;
    assert_eq!(received.len(), 1);
    let forwarded_bytes = received[0].body.clone();
    let forwarded_json: Value = serde_json::from_slice(&forwarded_bytes)?;
    assert_eq!(
        forwarded_json["system"][2]["text"].as_str(),
        Some(new_block2),
        "sanity: upstream must have seen the templated body"
    );

    let (request_body, filtered_request_body, manifest) =
        fetch_manifest_row(&pool, session_id).await?;
    let original_bytes = request_body.ok_or_else(|| anyhow::anyhow!("request_body missing"))?;
    assert!(
        filtered_request_body.is_none(),
        "the sent-body storage fix retires the full-body column: filtered_request_body \
         must stay NULL even when the composer templated"
    );

    assert_envelope_patch_is_mutation_sized(&pool, request_id, &original_bytes).await?;
    assert_sent_body_reconstructs_forwarded_bytes(
        &client,
        &inspect_url,
        request_id,
        &forwarded_bytes,
        &original_bytes,
    )
    .await?;

    let manifest = manifest.ok_or_else(|| anyhow::anyhow!("transformation_manifest missing"))?;
    assert_manifest_records_applied_composer_stage(&manifest, &original_bytes, &forwarded_bytes)?;

    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn continuation_request_stores_no_sent_body_and_only_unchanged_stages() -> anyhow::Result<()>
{
    let upstream = upstream::canned_json_upstream("msg_manifest_identity").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();
    let session_id = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeee2";
    let block2 = "identity-role-body-never-composed";

    let r1 = send_canonical_request(&client, &proxy_url, session_id, block2).await?;
    assert_eq!(r1.status(), 200);
    let r1_id = latest_request_id_for_session(&client, &inspect_url, session_id).await?;
    wait_for_request_detail(&client, &inspect_url, r1_id).await?;

    let r2 = send_canonical_request(&client, &proxy_url, session_id, block2).await?;
    assert_eq!(r2.status(), 200);
    let r2_id = latest_request_id_for_session(&client, &inspect_url, session_id).await?;
    assert_ne!(r2_id, r1_id, "second request must mint a new id");
    wait_for_request_detail(&client, &inspect_url, r2_id).await?;

    let received = upstream.received_requests().await;
    let received = received.ok_or_else(|| anyhow::anyhow!("no requests"))?;
    assert_eq!(received.len(), 2);
    let forwarded_bytes_r2 = received[1].body.clone();

    let (request_body, filtered_request_body, manifest) =
        fetch_manifest_row(&pool, session_id).await?;
    let original_bytes = request_body.ok_or_else(|| anyhow::anyhow!("request_body missing"))?;

    assert_eq!(
        original_bytes, forwarded_bytes_r2,
        "identity continuation: forwarded bytes must equal the client's original bytes"
    );
    assert!(
        filtered_request_body.is_none(),
        "identity continuation must not populate filtered_request_body"
    );

    let manifest = manifest.ok_or_else(|| anyhow::anyhow!("transformation_manifest missing"))?;
    let stages = manifest["stages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("stages missing"))?;
    assert!(
        stages.iter().all(|s| s["outcome"] == "unchanged"),
        "identity continuation must record only unchanged stages: {stages:?}"
    );
    assert_eq!(manifest["original_sha256"], manifest["sent_sha256"]);
    assert_manifest_chain_closes(&manifest)?;

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
