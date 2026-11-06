use crate::db_helpers;
use db_helpers::start_proxy_with_db;
use serde_json::Value;

use super::support::{mount_upstream, send_request};

async fn fetch_session_requests(
    client: &reqwest::Client,
    inspect_url: &str,
    session: &str,
    order: Option<&str>,
) -> anyhow::Result<Vec<Value>> {
    let url = match order {
        Some(o) => format!("{inspect_url}/api/sessions/{session}?order={o}"),
        None => format!("{inspect_url}/api/sessions/{session}"),
    };
    let v: Value = client.get(url).send().await?.json().await?;
    let rows = v
        .get("requests")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("requests must be an array"))?
        .clone();
    Ok(rows)
}

fn request_id_and_number(r: &Value) -> anyhow::Result<(u64, u64)> {
    let id = r
        .get("id")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow::anyhow!("missing id"))?;
    let n = r
        .get("request_number")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow::anyhow!("missing request_number"))?;
    Ok((id, n))
}

fn assert_desc_and_asc_share_absolute_request_numbers(
    desc_rows: &[Value],
    asc_rows: &[Value],
) -> anyhow::Result<()> {
    let desc_pairs: Vec<(u64, u64)> = desc_rows
        .iter()
        .map(request_id_and_number)
        .collect::<Result<_, _>>()?;
    assert!(
        desc_pairs.windows(2).all(|w| w[0].0 > w[1].0),
        "desc rows must be strictly newest-first by id: {desc_pairs:?}"
    );
    assert_eq!(
        desc_pairs.iter().map(|p| p.1).collect::<Vec<_>>(),
        vec![3, 2, 1],
        "request_number must stay the absolute chronological ordinal under desc"
    );

    let asc_pairs: Vec<(u64, u64)> = asc_rows
        .iter()
        .map(request_id_and_number)
        .collect::<Result<_, _>>()?;
    let mut mirrored = asc_pairs.clone();
    mirrored.reverse();
    assert_eq!(
        mirrored, desc_pairs,
        "order=asc must be the exact reverse of order=desc"
    );
    assert_eq!(
        asc_pairs.iter().map(|p| p.1).collect::<Vec<_>>(),
        vec![1, 2, 3],
        "request_number must be 1..=3 chronologically under asc"
    );
    Ok(())
}

#[tokio::test]
async fn get_session_detail_defaults_desc_and_keeps_absolute_request_number() -> anyhow::Result<()>
{
    let mock_server = mount_upstream().await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, _pool, test_db) = proxy.into_parts();

    let client = reqwest::Client::new();
    let session = uuid::Uuid::new_v4().to_string();
    for i in 0..3 {
        send_request(&client, &proxy_url, &session, &format!("u-{i}")).await?;
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    let default_rows = fetch_session_requests(&client, &inspect_url, &session, None).await?;
    let desc_rows = fetch_session_requests(&client, &inspect_url, &session, Some("desc")).await?;
    let asc_rows = fetch_session_requests(&client, &inspect_url, &session, Some("asc")).await?;
    assert_eq!(default_rows.len(), 3);
    assert_eq!(
        default_rows, desc_rows,
        "default must be the desc (newest-first) ordering"
    );

    assert_desc_and_asc_share_absolute_request_numbers(&desc_rows, &asc_rows)?;

    let raw: Value = client
        .get(format!("{inspect_url}/api/sessions/{session}?order=asc"))
        .send()
        .await?
        .json()
        .await?;
    assert!(
        raw.get("total_requests")
            .map(Value::is_null)
            .unwrap_or(true),
        "order alone must not trigger the paginated response shape"
    );

    let bad = client
        .get(format!(
            "{inspect_url}/api/sessions/{session}?order=chronological"
        ))
        .send()
        .await?;
    assert_eq!(bad.status(), reqwest::StatusCode::BAD_REQUEST);

    shutdown.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    test_db.cleanup().await?;
    Ok(())
}
