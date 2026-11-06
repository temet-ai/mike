use crate::db_helpers;
use db_helpers::start_proxy_with_db;
use serde_json::Value;
use sqlx::PgPool;

use super::support::{mount_upstream, send_request};

async fn seed_two_titled_sessions(
    client: &reqwest::Client,
    proxy_url: &str,
    pool: &PgPool,
) -> anyhow::Result<(String, String)> {
    let known_session = uuid::Uuid::new_v4().to_string();
    send_request(client, proxy_url, &known_session, "u").await?;
    sqlx::query("UPDATE sessions.sessions SET title = $1 WHERE session_uuid = $2::text::uuid")
        .bind("alpha-bravo-charlie")
        .bind(&known_session)
        .execute(pool)
        .await?;

    let other_session = uuid::Uuid::new_v4().to_string();
    send_request(client, proxy_url, &other_session, "u").await?;
    sqlx::query("UPDATE sessions.sessions SET title = $1 WHERE session_uuid = $2::text::uuid")
        .bind("zeta-yankee")
        .bind(&other_session)
        .execute(pool)
        .await?;

    Ok((known_session, other_session))
}

async fn assert_title_search_isolates_known_session(
    client: &reqwest::Client,
    inspect_url: &str,
    known_session: &str,
    other_session: &str,
) -> anyhow::Result<()> {
    let by_title: Value = client
        .get(format!("{inspect_url}/api/sessions?search=alpha&limit=200"))
        .send()
        .await?
        .json()
        .await?;
    let by_title_arr = by_title
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("not array"))?;
    let title_ids: Vec<String> = by_title_arr
        .iter()
        .filter_map(|r| r.get("id").and_then(|v| v.as_str().map(str::to_string)))
        .collect();
    assert!(
        title_ids.contains(&known_session.to_string()),
        "search='alpha' must include the session whose title contains 'alpha'"
    );
    assert!(
        !title_ids.contains(&other_session.to_string()),
        "search='alpha' must NOT include the unrelated session ('zeta-yankee')"
    );

    for row in by_title_arr {
        let avg = row
            .get("avg_latency_ms")
            .ok_or_else(|| anyhow::anyhow!("search row missing avg_latency_ms key"))?;
        assert!(
            avg.is_null(),
            "search row avg_latency_ms must be null (latency is detail-only), got {avg:?}"
        );
    }
    Ok(())
}

async fn assert_id_search_is_exact_full_uuid_only(
    client: &reqwest::Client,
    inspect_url: &str,
    known_session: &str,
    other_session: &str,
) -> anyhow::Result<()> {
    let by_id: Value = client
        .get(format!(
            "{inspect_url}/api/sessions?search={known_session}&limit=200"
        ))
        .send()
        .await?
        .json()
        .await?;
    let by_id_arr = by_id
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("not array"))?;
    let id_match_ids: Vec<String> = by_id_arr
        .iter()
        .filter_map(|r| r.get("id").and_then(|v| v.as_str().map(str::to_string)))
        .collect();
    assert!(
        id_match_ids.contains(&known_session.to_string()),
        "full session_id '{known_session}' must hit the session with that id"
    );
    assert!(
        !id_match_ids.contains(&other_session.to_string()),
        "exact session_id search must NOT return unrelated sessions"
    );

    let prefix: String = known_session.chars().take(8).collect();
    let by_prefix: Value = client
        .get(format!(
            "{inspect_url}/api/sessions?search={prefix}&limit=200"
        ))
        .send()
        .await?
        .json()
        .await?;
    let by_prefix_ids: Vec<String> = by_prefix
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("not array"))?
        .iter()
        .filter_map(|r| r.get("id").and_then(|v| v.as_str().map(str::to_string)))
        .collect();
    assert!(
        !by_prefix_ids.contains(&known_session.to_string()),
        "an 8-char session_id prefix must NOT match (exact full-UUID only)"
    );
    Ok(())
}

async fn assert_empty_search_equals_unfiltered(
    client: &reqwest::Client,
    inspect_url: &str,
) -> anyhow::Result<()> {
    let empty_search: Value = client
        .get(format!("{inspect_url}/api/sessions?search=&limit=200"))
        .send()
        .await?
        .json()
        .await?;
    let empty_arr = empty_search
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("not array"))?;
    let unfiltered: Value = client
        .get(format!("{inspect_url}/api/sessions?limit=200"))
        .send()
        .await?
        .json()
        .await?;
    let unfiltered_arr = unfiltered
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("not array"))?;
    assert_eq!(
        empty_arr.len(),
        unfiltered_arr.len(),
        "?search= empty must behave as filter absent"
    );
    Ok(())
}

#[tokio::test]
async fn list_sessions_search_matches_title_and_session_id() -> anyhow::Result<()> {
    let mock_server = mount_upstream().await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let client = reqwest::Client::new();

    let (known_session, other_session) =
        seed_two_titled_sessions(&client, &proxy_url, &pool).await?;

    assert_title_search_isolates_known_session(
        &client,
        &inspect_url,
        &known_session,
        &other_session,
    )
    .await?;
    assert_id_search_is_exact_full_uuid_only(&client, &inspect_url, &known_session, &other_session)
        .await?;
    assert_empty_search_equals_unfiltered(&client, &inspect_url).await?;

    shutdown.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
