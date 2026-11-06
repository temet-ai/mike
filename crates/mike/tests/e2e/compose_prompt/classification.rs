use crate::common;
use crate::db_helpers;
use crate::upstream;

use serde_json::Value;

use super::support::{
    insert_composition, latest_request_id_for_session, prompt_sha256, send_canonical_request,
    send_string_form_request, send_two_block_request,
};

#[tokio::test]
async fn classifies_main_agent_from_block_2_text() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose").await;
    let (proxy_url, inspect_url) =
        common::start_proxy_with_composition_enabled(&upstream.uri()).await?;
    let client = reqwest::Client::new();
    let session_id = "8ec5b1ea-1234-4567-8901-aaaaaaaaaaaa";
    let block2 = "main-agent-role-body-v1";

    let resp = send_canonical_request(&client, &proxy_url, session_id, block2).await?;
    assert_eq!(resp.status(), 200);

    let request_id = latest_request_id_for_session(&client, &inspect_url, session_id).await?;
    let detail = super::support::wait_for_request_detail(&client, &inspect_url, request_id).await?;
    let projection = detail
        .get("system_blocks")
        .ok_or_else(|| anyhow::anyhow!("missing system_blocks"))?;
    let actual_prompt_sha256 = projection
        .get("prompt_sha256")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("prompt_sha256 absent"))?;
    assert_eq!(actual_prompt_sha256, prompt_sha256(block2));
    let blocks = projection
        .get("blocks")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("blocks absent"))?;
    assert_eq!(blocks.len(), 3);
    assert_eq!(blocks[2].get("text").and_then(|t| t.as_str()), Some(block2));
    assert_eq!(detail.get("composition_applied"), Some(&Value::Bool(false)));
    assert_eq!(detail.get("unknown_blocks"), Some(&Value::Null));
    Ok(())
}

#[tokio::test]
async fn passthrough_when_not_first_in_session() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (proxy_url, _inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();
    let session_id = "be57bdc1-cccc-4444-8888-bbbbbbbbbbbb";
    let block2 = "second-request-role-body";

    insert_composition(
        &pool,
        &prompt_sha256(block2),
        "test",
        "REWRITTEN",
        "template",
    )
    .await?;

    let r1 = send_canonical_request(&client, &proxy_url, session_id, block2).await?;
    assert_eq!(r1.status(), 200);
    let r2 = send_canonical_request(&client, &proxy_url, session_id, block2).await?;
    assert_eq!(r2.status(), 200);

    let received = upstream.received_requests().await;
    let received = received.ok_or_else(|| anyhow::anyhow!("no requests"))?;
    assert_eq!(received.len(), 2);
    for req in &received {
        let body: Value = serde_json::from_slice(&req.body)?;
        assert_eq!(
            body["system"][2]["text"].as_str(),
            Some(block2),
            "body must not have been rewritten on either request"
        );
    }
    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn passthrough_when_class_unknown() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose").await;
    let (proxy_url, inspect_url) =
        common::start_proxy_with_composition_enabled(&upstream.uri()).await?;
    let client = reqwest::Client::new();
    let session_id = "12345678-aaaa-4bbb-8ccc-dddddddddddd";
    let block2 = "completely novel string";

    let r = send_canonical_request(&client, &proxy_url, session_id, block2).await?;
    assert_eq!(r.status(), 200);

    let request_id = latest_request_id_for_session(&client, &inspect_url, session_id).await?;
    let detail = super::support::wait_for_request_detail(&client, &inspect_url, request_id).await?;
    let projection = detail
        .get("system_blocks")
        .ok_or_else(|| anyhow::anyhow!("missing system_blocks"))?;
    let actual = projection
        .get("prompt_sha256")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("prompt_sha256 absent"))?;
    assert_eq!(actual, prompt_sha256(block2));
    assert_eq!(detail.get("composition_applied"), Some(&Value::Bool(false)));
    let received = upstream.received_requests().await;
    let received = received.ok_or_else(|| anyhow::anyhow!("no requests"))?;
    assert_eq!(received.len(), 1);
    let upstream_body: Value = serde_json::from_slice(&received[0].body)?;
    assert_eq!(upstream_body["system"][2]["text"].as_str(), Some(block2));
    Ok(())
}

#[tokio::test]
async fn passthrough_on_string_form_system() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose").await;
    let (proxy_url, inspect_url) = common::start_proxy(&upstream.uri()).await?;
    let client = reqwest::Client::new();
    let session_id = "abcdef01-2345-4678-89ab-cdef01234567";

    let r = send_string_form_request(&client, &proxy_url, session_id).await?;
    assert_eq!(r.status(), 200);

    let request_id = latest_request_id_for_session(&client, &inspect_url, session_id).await?;
    let detail = super::support::wait_for_request_detail(&client, &inspect_url, request_id).await?;
    assert!(
        detail
            .get("unknown_blocks")
            .is_none_or(serde_json::Value::is_null),
        "string-form system resolves an identity, so it is not an anomaly: {detail:?}"
    );
    assert_eq!(detail.get("composition_applied"), Some(&Value::Bool(false)));
    Ok(())
}

#[tokio::test]
async fn passthrough_on_non_3_block_shape() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose").await;
    let (proxy_url, inspect_url) = common::start_proxy(&upstream.uri()).await?;
    let client = reqwest::Client::new();
    let session_id = "11223344-aaaa-4bbb-8ccc-555566667777";

    let r = send_two_block_request(&client, &proxy_url, session_id).await?;
    assert_eq!(r.status(), 200);

    let request_id = latest_request_id_for_session(&client, &inspect_url, session_id).await?;
    let detail = super::support::wait_for_request_detail(&client, &inspect_url, request_id).await?;
    assert!(
        detail
            .get("unknown_blocks")
            .is_none_or(serde_json::Value::is_null),
        "a 2-block system resolves an identity, so it is not an anomaly: {detail:?}"
    );
    Ok(())
}
