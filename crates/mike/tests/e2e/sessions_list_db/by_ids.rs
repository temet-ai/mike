use crate::db_helpers;
use db_helpers::start_proxy_with_db;
use serde_json::Value;

use super::support::{mount_upstream, send_request};

#[tokio::test]
async fn list_sessions_by_ids_returns_exactly_those_sessions_full_shape() -> anyhow::Result<()> {
    let mock_server = mount_upstream().await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, _pool, test_db) = proxy.into_parts();

    let client = reqwest::Client::new();

    let mut session_ids: Vec<String> = Vec::new();
    for i in 0..4 {
        let sid = uuid::Uuid::new_v4().to_string();
        send_request(&client, &proxy_url, &sid, &format!("ids-{i}")).await?;
        session_ids.push(sid);
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    let ids_param = format!("{},{}", session_ids[0], session_ids[2]);
    let resp = client
        .get(format!("{}/api/sessions", &inspect_url))
        .query(&[("ids", ids_param.as_str())])
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: Value = resp.json().await?;
    let arr = body
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("not array"))?;
    assert_eq!(
        arr.len(),
        2,
        "must return exactly the two requested sessions"
    );

    let returned_ids: Vec<&str> = arr
        .iter()
        .filter_map(|r| r.get("id").and_then(Value::as_str))
        .collect();
    assert_eq!(
        returned_ids,
        vec![session_ids[2].as_str(), session_ids[0].as_str()],
        "must be ordered last_seen DESC, not by ids= request order"
    );
    assert!(!returned_ids.contains(&session_ids[1].as_str()));
    assert!(!returned_ids.contains(&session_ids[3].as_str()));

    for row in arr {
        assert_eq!(row.get("request_count").and_then(Value::as_i64), Some(1));
        assert_eq!(
            row.get("pending_inbox_count").and_then(Value::as_i64),
            Some(0)
        );
        assert!(row.get("last_status").is_some());
        assert!(
            row.get("avg_latency_ms")
                .map(|v| v.is_null() || v.is_number())
                .unwrap_or(false)
        );
    }

    shutdown.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn list_sessions_by_ids_unknown_id_omitted_and_duplicates_dedupe() -> anyhow::Result<()> {
    let mock_server = mount_upstream().await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, _pool, test_db) = proxy.into_parts();

    let client = reqwest::Client::new();
    let known = uuid::Uuid::new_v4().to_string();
    send_request(&client, &proxy_url, &known, "known").await?;

    let unknown = uuid::Uuid::new_v4().to_string();
    let ids_param = format!("{known},{unknown},{known}");
    let resp = client
        .get(format!("{}/api/sessions", &inspect_url))
        .query(&[("ids", ids_param.as_str())])
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: Value = resp.json().await?;
    let arr = body
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("not array"))?;
    assert_eq!(
        arr.len(),
        1,
        "unknown id must be silently absent and the duplicate must dedupe, got {arr:?}"
    );
    assert_eq!(
        arr[0].get("id").and_then(Value::as_str),
        Some(known.as_str())
    );

    shutdown.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn list_sessions_by_ids_malformed_uuid_returns_400() -> anyhow::Result<()> {
    let mock_server = mount_upstream().await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, _pool, test_db) = proxy.into_parts();

    let client = reqwest::Client::new();
    let valid = uuid::Uuid::new_v4().to_string();
    let ids_param = format!("{valid},not-a-uuid");
    let resp = client
        .get(format!("{}/api/sessions", &inspect_url))
        .query(&[("ids", ids_param.as_str())])
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);

    shutdown.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn list_sessions_by_ids_over_cap_returns_400() -> anyhow::Result<()> {
    let mock_server = mount_upstream().await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, _pool, test_db) = proxy.into_parts();

    let client = reqwest::Client::new();
    let ids_param = (0..201)
        .map(|_| uuid::Uuid::new_v4().to_string())
        .collect::<Vec<_>>()
        .join(",");
    let resp = client
        .get(format!("{}/api/sessions", &inspect_url))
        .query(&[("ids", ids_param.as_str())])
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);

    shutdown.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    test_db.cleanup().await?;
    Ok(())
}
