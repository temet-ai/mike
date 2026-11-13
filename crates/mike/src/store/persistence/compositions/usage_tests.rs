//! Composition usage stats, over compositions this crate creates.
use crate::store::persistence::proxy_repo::ProxyPersist as _;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::compositions::NewCompositionParams;
use crate::store::persistence::request_write::IdMode;
use crate::store::persistence::test_support as test_util;

use crate::store::models::{InterceptedRequest, RequestStatus};

async fn snapshot_via_handle(
    handle: &PersistHandle,
    session_id: &str,
    memory_id: u64,
    body_marker: &str,
    configure: impl FnOnce(&mut InterceptedRequest),
) -> anyhow::Result<()> {
    let mut req = crate::store::tests::make_test_request(
        Some(crate::store::models::RequestId::from_raw(memory_id)),
        session_id,
    );
    req.status = RequestStatus::Complete;
    req.request_body = Some(bytes::Bytes::from(format!("req-body-{body_marker}")));
    req.response_body = Some(bytes::Bytes::from(format!("resp-body-{body_marker}")));
    req.completed_at = Some(req.created_at);
    configure(&mut req);
    handle
        .insert_request_initial(&req, true, IdMode::Ensure)
        .await?;
    handle.snapshot_request(&req).await?;
    Ok(())
}

#[tokio::test]
async fn composition_usage_stats_returns_totals_and_recent_in_one_round_trip() -> anyhow::Result<()>
{
    let Some(db) = test_util::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;

    let session_id = uuid::Uuid::new_v4().to_string();
    let sha = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let blocks = serde_json::json!([{ "type": "text", "text": "hello" }]);
    let composition = handle
        .proxy()
        .create_composition(NewCompositionParams {
            prompt_sha256: &sha,
            label: "usage-stats-test",
            blocks: &blocks,
            mode: None,
            agent_name: None,
            agent_definition_path: None,
        })
        .await?;

    let base = chrono::Utc::now();
    for i in 0..5u64 {
        let created = base + chrono::Duration::milliseconds(i as i64 * 10);
        let sha = sha.clone();
        let blocks = blocks.clone();
        snapshot_via_handle(&handle, &session_id, 50_000 + i, "u", move |req| {
            req.created_at = created;
            req.completed_at = Some(created);
            req.system_prompt.prompt_sha256 = Some(sha);
            req.system_prompt.blocks = Some(blocks);
            req.token_usage = Some(crate::types::TokenUsage {
                input_tokens: 100 + (i as u32) * 10,
                output_tokens: 200 + (i as u32) * 10,
                cache_read_input_tokens: None,
                cache_creation_input_tokens: None,
            });
        })
        .await?;
    }

    let stats = handle.composition_usage_stats(composition.id, 3).await?;
    assert_eq!(stats.count_total, 5, "all 5 seeded rows must be counted");
    assert_eq!(stats.count_1h, 5);
    assert_eq!(stats.count_24h, 5);
    assert_eq!(stats.count_7d, 5);
    assert_eq!(stats.total_input_tokens, 600);
    assert_eq!(stats.total_output_tokens, 1100);
    assert_eq!(
        stats.recent.len(),
        3,
        "limit=3 must cap the recent list regardless of window count"
    );
    assert_eq!(stats.recent[0].input_tokens, Some(140));
    assert_eq!(stats.recent[1].input_tokens, Some(130));
    assert_eq!(stats.recent[2].input_tokens, Some(120));

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn composition_usage_stats_returns_zero_and_empty_for_unknown_composition()
-> anyhow::Result<()> {
    let Some(db) = test_util::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;

    let stats = handle.composition_usage_stats(i64::MAX, 50).await?;
    assert_eq!(stats.count_total, 0);
    assert_eq!(stats.count_1h, 0);
    assert_eq!(stats.count_24h, 0);
    assert_eq!(stats.count_7d, 0);
    assert_eq!(stats.total_input_tokens, 0);
    assert_eq!(stats.total_output_tokens, 0);
    assert!(stats.recent.is_empty(), "empty recent list expected");

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn count_applied_24h_counts_only_override_applied_requests_per_composition()
-> anyhow::Result<()> {
    let Some(db) = test_util::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;

    let session_id = uuid::Uuid::new_v4().to_string();
    let sha = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let blocks = serde_json::json!([{ "type": "text", "text": "hello" }]);
    let composition = handle
        .proxy()
        .create_composition(NewCompositionParams {
            prompt_sha256: &sha,
            label: "applied-count-test",
            blocks: &blocks,
            mode: None,
            agent_name: None,
            agent_definition_path: None,
        })
        .await?;

    for (i, applied) in [true, true, false].into_iter().enumerate() {
        let sha = sha.clone();
        let blocks = blocks.clone();
        let composition_id = composition.id;
        snapshot_via_handle(&handle, &session_id, 60_000 + i as u64, "a", move |req| {
            req.system_prompt.prompt_sha256 = Some(sha);
            req.system_prompt.blocks = Some(blocks);
            req.system_prompt.composition_id = Some(composition_id);
            req.system_prompt.composition_applied = applied;
        })
        .await?;
    }

    assert_eq!(handle.count_applied_24h(composition.id).await?, 2);
    let all = handle.count_applied_24h_all().await?;
    assert_eq!(all.get(&composition.id).copied(), Some(2));

    db.cleanup().await?;
    Ok(())
}
