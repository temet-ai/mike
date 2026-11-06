use crate::db_helpers;
use db_helpers::start_proxy_with_db;
use serde_json::Value;

use super::support::{mount_upstream, send_request};

#[tokio::test]
async fn list_sessions_paginates_and_orders_by_last_seen_desc() -> anyhow::Result<()> {
    let mock_server = mount_upstream().await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, _pool, test_db) = proxy.into_parts();

    let client = reqwest::Client::new();

    let mut session_ids: Vec<String> = Vec::new();
    for i in 0..5 {
        let sid = uuid::Uuid::new_v4().to_string();
        send_request(&client, &proxy_url, &sid, &format!("u-{i}")).await?;
        session_ids.push(sid);
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    let p1: Value = client
        .get(format!("{}/api/sessions?offset=0&limit=2", &inspect_url))
        .send()
        .await?
        .json()
        .await?;
    let p1_arr = p1.as_array().ok_or_else(|| anyhow::anyhow!("not array"))?;
    assert_eq!(p1_arr.len(), 2, "page 1 must have exactly 2 rows");

    let p2: Value = client
        .get(format!("{}/api/sessions?offset=2&limit=2", &inspect_url))
        .send()
        .await?
        .json()
        .await?;
    let p2_arr = p2.as_array().ok_or_else(|| anyhow::anyhow!("not array"))?;
    assert_eq!(p2_arr.len(), 2);

    let ids1: Vec<String> = p1_arr
        .iter()
        .filter_map(|r| r.get("id").and_then(|v| v.as_str().map(str::to_string)))
        .collect();
    let ids2: Vec<String> = p2_arr
        .iter()
        .filter_map(|r| r.get("id").and_then(|v| v.as_str().map(str::to_string)))
        .collect();
    for id in &ids2 {
        assert!(!ids1.contains(id), "pages overlap on session id {id}");
    }

    let all: Value = client
        .get(format!("{}/api/sessions?limit=200", &inspect_url))
        .send()
        .await?
        .json()
        .await?;
    let all_arr = all.as_array().ok_or_else(|| anyhow::anyhow!("not array"))?;
    for window in all_arr.windows(2) {
        let a = window[0]
            .get("last_seen")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing last_seen"))?;
        let b = window[1]
            .get("last_seen")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing last_seen"))?;
        assert!(a >= b, "DESC ordering broken: {a} < {b}");
    }

    for row in all_arr {
        let avg = row
            .get("avg_latency_ms")
            .ok_or_else(|| anyhow::anyhow!("list row missing avg_latency_ms key"))?;
        assert!(
            avg.is_null(),
            "list row avg_latency_ms must be null (latency is detail-only), got {avg:?}"
        );
    }

    shutdown.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn list_sessions_order_asc_is_exact_mirror_of_default_desc() -> anyhow::Result<()> {
    let mock_server = mount_upstream().await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, _pool, test_db) = proxy.into_parts();

    let client = reqwest::Client::new();
    for i in 0..3 {
        let sid = uuid::Uuid::new_v4().to_string();
        send_request(&client, &proxy_url, &sid, &format!("u-{i}")).await?;
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    let ids = |v: &Value| -> anyhow::Result<Vec<String>> {
        v.as_array()
            .ok_or_else(|| anyhow::anyhow!("not array"))?
            .iter()
            .map(|r| {
                r.get("id")
                    .and_then(|v| v.as_str().map(str::to_string))
                    .ok_or_else(|| anyhow::anyhow!("missing id"))
            })
            .collect()
    };

    let desc: Value = client
        .get(format!(
            "{}/api/sessions?limit=200&order=desc",
            &inspect_url
        ))
        .send()
        .await?
        .json()
        .await?;
    let default_page: Value = client
        .get(format!("{}/api/sessions?limit=200", &inspect_url))
        .send()
        .await?
        .json()
        .await?;
    let asc: Value = client
        .get(format!("{}/api/sessions?limit=200&order=asc", &inspect_url))
        .send()
        .await?
        .json()
        .await?;

    let desc_ids = ids(&desc)?;
    assert_eq!(
        ids(&default_page)?,
        desc_ids,
        "explicit order=desc must equal the default ordering"
    );
    let mut asc_ids = ids(&asc)?;
    asc_ids.reverse();
    assert_eq!(
        asc_ids, desc_ids,
        "order=asc must be the exact reverse of order=desc"
    );

    let bad = client
        .get(format!("{}/api/sessions?order=newest", &inspect_url))
        .send()
        .await?;
    assert_eq!(bad.status(), reqwest::StatusCode::BAD_REQUEST);

    shutdown.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    test_db.cleanup().await?;
    Ok(())
}
