use crate::db_helpers;

use serde_json::{Value, json};
use wiremock::MockServer;

use super::support::{ensure_session, seed_live_registration, seed_role};

#[tokio::test]
async fn role_fanout_n1_keeps_delivery_shape_and_isolates_by_project() -> anyhow::Result<()> {
    let upstream = MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping mailbox_tags_addressing (N=1 shape)");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let sender = "dd000000-0000-4000-e007-000000000001";
    let target = "dd000000-0000-4000-e008-000000000001";
    let other_project_peer = "dd000000-0000-4000-e009-000000000001";
    let project_path = "mailbox-role-n1-test";
    let other_project_path = "mailbox-role-n1-other-project-test";

    let (sender_id, project_id) = ensure_session(&pool, sender, project_path).await?;
    let (target_id, _) = ensure_session(&pool, target, project_path).await?;
    let (other_id, other_project_id) =
        ensure_session(&pool, other_project_peer, other_project_path).await?;

    seed_live_registration(&pool, sender_id, project_id).await?;
    seed_live_registration(&pool, target_id, project_id).await?;
    seed_live_registration(&pool, other_id, other_project_id).await?;
    seed_role(&pool, target_id, "release-manager").await?;
    seed_role(&pool, other_id, "release-manager").await?;

    let client = reqwest::Client::new();
    let fanout_resp = client
        .post(format!(
            "{inspect_url}/api/sessions/role:release-manager/message"
        ))
        .json(&json!({
            "message": "single-recipient fan-out",
            "from_session_id": sender,
            "subject": "n1 shape check",
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
        "N=1 fan-out must keep the {{thread_uuid, subject, deliveries}} shape: got {fanout_keys:?}"
    );
    let deliveries = fanout["deliveries"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("deliveries not an array: {fanout}"))?;
    anyhow::ensure!(
        deliveries.len() == 1,
        "exactly one in-project session declares release-manager: got {} deliveries: {fanout}",
        deliveries.len()
    );
    anyhow::ensure!(
        deliveries[0]["to_session_uuid"].as_str() == Some(target),
        "the single delivery must address the in-project session: {fanout}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
