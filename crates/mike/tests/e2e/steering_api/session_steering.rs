use crate::db_helpers;
use serde_json::Value;

use super::support::{
    fire_ensures_session_exists, fire_request_and_get_id, mount_minimal_upstream, seed_intervention,
};

#[tokio::test]
async fn get_session_steering_returns_empty_for_new_session() -> anyhow::Result<()> {
    let upstream = mount_minimal_upstream("msg_test_session_steering").await;

    let Some(env) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset — skipping \
             get_session_steering_returns_empty_for_new_session"
        );
        return Ok(());
    };

    let client = reqwest::Client::new();
    let session_uuid = "c0ffee00-0000-0000-0000-000000000002";

    fire_ensures_session_exists(
        &client,
        &env.proxy_url,
        session_uuid,
        "steering api test — session steering empty",
    )
    .await?;

    let steering_resp = client
        .get(format!(
            "{}/api/sessions/{}/steering",
            env.inspect_url, session_uuid
        ))
        .send()
        .await?;
    assert_eq!(
        steering_resp.status(),
        reqwest::StatusCode::OK,
        "expected 200 for session steering; got {}",
        steering_resp.status()
    );
    let body: Value = steering_resp.json().await?;
    assert_eq!(
        body["rows"],
        serde_json::json!([]),
        "expected empty rows page for session with no interventions; got {body}"
    );
    assert_eq!(body["total_count"], 0, "expected total_count 0; got {body}");

    env.shutdown.cancel();
    env.test_db.cleanup().await.ok();
    Ok(())
}

#[tokio::test]
async fn get_session_steering_paginates_with_offset_and_limit() -> anyhow::Result<()> {
    let upstream = mount_minimal_upstream("msg_test_session_steering_page").await;

    let Some(env) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset — skipping \
             get_session_steering_paginates_with_offset_and_limit"
        );
        return Ok(());
    };

    let client = reqwest::Client::new();
    let session_uuid = "c0ffee00-0000-0000-0000-000000000004";

    let mut seeded_anchors = Vec::new();
    for n in 0..3 {
        let trigger_request_id = fire_request_and_get_id(
            &client,
            &env.proxy_url,
            &env.pool,
            session_uuid,
            &format!("session steering pagination test prompt {n}"),
        )
        .await?;
        let anchor_text = format!("pagination-row-{n}");
        seed_intervention(&env.pool, session_uuid, trigger_request_id, &anchor_text).await?;
        seeded_anchors.push(anchor_text);
    }

    let mut seen_anchors = Vec::new();
    for offset in [0, 2] {
        let resp = client
            .get(format!(
                "{}/api/sessions/{}/steering?offset={}&limit=2",
                env.inspect_url, session_uuid, offset
            ))
            .send()
            .await?;
        assert_eq!(
            resp.status(),
            reqwest::StatusCode::OK,
            "expected 200 for session steering page; got {}",
            resp.status()
        );
        let body: Value = resp.json().await?;
        assert_eq!(body["total_count"], 3, "expected total_count 3; got {body}");
        assert_eq!(body["offset"], offset, "expected echoed offset; got {body}");
        assert_eq!(body["limit"], 2, "expected echoed limit; got {body}");
        let items = body["rows"]
            .as_array()
            .unwrap_or_else(|| panic!("rows must be an array; got {body}"));
        let expected_len = if offset == 0 { 2 } else { 1 };
        assert_eq!(
            items.len(),
            expected_len,
            "expected {expected_len} items at offset {offset}; got {body}"
        );
        for item in items {
            seen_anchors.push(
                item["anchor_text"]
                    .as_str()
                    .unwrap_or_else(|| panic!("anchor_text must be present; got {item}"))
                    .to_string(),
            );
        }
    }
    seen_anchors.sort();
    let mut expected = seeded_anchors;
    expected.sort();
    assert_eq!(
        seen_anchors, expected,
        "the two pages together must cover every seeded intervention exactly once"
    );

    env.shutdown.cancel();
    env.test_db.cleanup().await.ok();
    Ok(())
}

#[tokio::test]
async fn get_session_steering_clamps_limit_to_valid_range() -> anyhow::Result<()> {
    let upstream = mount_minimal_upstream("msg_test_session_steering_limit_clamp").await;

    let Some(env) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset — skipping \
             get_session_steering_clamps_limit_to_valid_range"
        );
        return Ok(());
    };

    let client = reqwest::Client::new();
    let session_uuid = "c0ffee00-0000-0000-0000-000000000005";

    fire_ensures_session_exists(
        &client,
        &env.proxy_url,
        session_uuid,
        "steering api test — limit clamp",
    )
    .await?;

    for (requested_limit, expected_limit) in [(0, 1), (99_999, 500)] {
        let steering_resp = client
            .get(format!(
                "{}/api/sessions/{}/steering?limit={}",
                env.inspect_url, session_uuid, requested_limit
            ))
            .send()
            .await?;
        assert_eq!(
            steering_resp.status(),
            reqwest::StatusCode::OK,
            "expected 200 for requested_limit={requested_limit}; got {}",
            steering_resp.status()
        );
        let body: Value = steering_resp.json().await?;
        assert_eq!(
            body["limit"], expected_limit,
            "requested_limit={requested_limit} must clamp to {expected_limit}; got {body}"
        );
    }

    env.shutdown.cancel();
    env.test_db.cleanup().await.ok();
    Ok(())
}

#[tokio::test]
async fn get_session_steering_offset_past_total_count_returns_empty_page() -> anyhow::Result<()> {
    let upstream = mount_minimal_upstream("msg_test_session_steering_offset_overflow").await;

    let Some(env) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset — skipping \
             get_session_steering_offset_past_total_count_returns_empty_page"
        );
        return Ok(());
    };

    let client = reqwest::Client::new();
    let session_uuid = "c0ffee00-0000-0000-0000-000000000006";

    let trigger_request_id = fire_request_and_get_id(
        &client,
        &env.proxy_url,
        &env.pool,
        session_uuid,
        "session steering offset-overflow test prompt",
    )
    .await?;
    seed_intervention(
        &env.pool,
        session_uuid,
        trigger_request_id,
        "offset-overflow-row",
    )
    .await?;

    for offset in [1_000, usize::MAX] {
        let steering_resp = client
            .get(format!(
                "{}/api/sessions/{}/steering?offset={}",
                env.inspect_url, session_uuid, offset
            ))
            .send()
            .await?;
        assert_eq!(
            steering_resp.status(),
            reqwest::StatusCode::OK,
            "expected 200 (not 500) for offset={offset}; got {}",
            steering_resp.status()
        );
        let body: Value = steering_resp.json().await?;
        assert_eq!(
            body["rows"],
            serde_json::json!([]),
            "expected empty rows page for offset={offset} past total_count; got {body}"
        );
        assert_eq!(
            body["total_count"], 1,
            "expected total_count 1 (the one seeded row) for offset={offset}; got {body}"
        );
    }

    env.shutdown.cancel();
    env.test_db.cleanup().await.ok();
    Ok(())
}
