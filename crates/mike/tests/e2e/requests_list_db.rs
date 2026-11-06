use crate::anthropic_fixtures;
use crate::db_helpers;

use anthropic_fixtures::{MockResponse, RequestBody, text_block};
use db_helpers::start_proxy_with_db;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn send_request(
    client: &reqwest::Client,
    proxy_url: &str,
    session_id: &str,
    system_text: &str,
    user_text: &str,
) -> anyhow::Result<()> {
    let body = RequestBody::default()
        .system(json!([
            text_block("static-1"),
            text_block("static-2"),
            text_block(system_text),
        ]))
        .content(user_text)
        .max_tokens(4)
        .build();
    let response = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-claude-code-session-id", session_id)
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await?;
    anyhow::ensure!(
        response.status() == 200,
        "expected 200 OK, got {}",
        response.status()
    );
    let _ = response.bytes().await?;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    Ok(())
}

async fn get_requests_json(client: &reqwest::Client, url: &str) -> anyhow::Result<Vec<Value>> {
    let body: Value = client.get(url).send().await?.json().await?;
    body.as_array()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("response body not an array: {body}"))
}

async fn mount_ok_response(mock_server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(
                    MockResponse::new("msg_a3_requests_list")
                        .text("ok")
                        .usage(1, 1)
                        .build(),
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(mock_server)
        .await;
}

#[tokio::test]
async fn list_requests_paginates_offset_and_limit() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    mount_ok_response(&mock_server).await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, _pool, test_db) = proxy.into_parts();

    let session = uuid::Uuid::new_v4().to_string();
    let client = reqwest::Client::new();

    for i in 0..5 {
        send_request(
            &client,
            &proxy_url,
            &session,
            &format!("system-text-{i}"),
            &format!("user-text-{i}"),
        )
        .await?;
    }

    let page1 = get_requests_json(
        &client,
        &format!("{inspect_url}/api/requests?offset=0&limit=2"),
    )
    .await?;
    assert_eq!(page1.len(), 2, "page1 must have exactly 2 rows");

    let page2 = get_requests_json(
        &client,
        &format!("{inspect_url}/api/requests?offset=2&limit=2"),
    )
    .await?;
    assert_eq!(page2.len(), 2, "page2 must have exactly 2 rows");

    let ids1: Vec<u64> = page1
        .iter()
        .filter_map(|r| r.get("id").and_then(|v| v.as_u64()))
        .collect();
    let ids2: Vec<u64> = page2
        .iter()
        .filter_map(|r| r.get("id").and_then(|v| v.as_u64()))
        .collect();
    for id in &ids2 {
        assert!(
            !ids1.contains(id),
            "pages must not overlap; id {id} appears in both"
        );
    }

    let last_of_page1 = page1[1]
        .get("created_at")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing created_at"))?;
    let first_of_page2 = page2[0]
        .get("created_at")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing created_at"))?;
    assert!(
        last_of_page1 >= first_of_page2,
        "newest-first ordering violated: page1[last]={last_of_page1} < page2[first]={first_of_page2}"
    );

    let big = get_requests_json(&client, &format!("{inspect_url}/api/requests?limit=500")).await?;
    assert!(
        big.len() <= 200,
        "limit=500 must clamp to <=200, got {}",
        big.len()
    );

    shutdown.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    test_db.cleanup().await?;
    Ok(())
}

async fn get_page(
    client: &reqwest::Client,
    url: &str,
) -> anyhow::Result<(Vec<Value>, Option<String>)> {
    let response = client.get(url).send().await?;
    anyhow::ensure!(response.status() == 200, "{url}: {}", response.status());
    let next = response
        .headers()
        .get("x-next-cursor")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let body: Value = response.json().await?;
    let rows = body
        .as_array()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("response body not an array: {body}"))?;
    Ok((rows, next))
}

fn ids_of(rows: &[Value]) -> Vec<u64> {
    rows.iter()
        .filter_map(|r| r.get("id").and_then(|v| v.as_u64()))
        .collect()
}

#[tokio::test]
async fn list_requests_pages_by_cursor_without_overlap_and_keeps_offset() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    mount_ok_response(&mock_server).await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, _pool, test_db) = proxy.into_parts();

    let session = uuid::Uuid::new_v4().to_string();
    let client = reqwest::Client::new();
    for i in 0..5 {
        send_request(
            &client,
            &proxy_url,
            &session,
            "cursor-sys",
            &format!("u-{i}"),
        )
        .await?;
    }

    let (page1, next1) = get_page(&client, &format!("{inspect_url}/api/requests?limit=2")).await?;
    let next1 = next1.ok_or_else(|| anyhow::anyhow!("a full page carries x-next-cursor"))?;
    let (page2, next2) = get_page(
        &client,
        &format!("{inspect_url}/api/requests?limit=2&cursor={next1}"),
    )
    .await?;
    let next2 = next2.ok_or_else(|| anyhow::anyhow!("page 2 is full, so it carries a cursor"))?;
    let (page3, next3) = get_page(
        &client,
        &format!("{inspect_url}/api/requests?limit=2&cursor={next2}&offset=4"),
    )
    .await?;
    assert_eq!((page1.len(), page2.len(), page3.len()), (2, 2, 1));
    assert!(next3.is_none(), "the last page carries no cursor");

    let by_cursor: Vec<u64> = [ids_of(&page1), ids_of(&page2), ids_of(&page3)].concat();
    let (all, _) = get_page(&client, &format!("{inspect_url}/api/requests?limit=10")).await?;
    assert_eq!(
        by_cursor,
        ids_of(&all),
        "cursor pages walk the same order as one page"
    );

    let (by_offset, _) = get_page(
        &client,
        &format!("{inspect_url}/api/requests?offset=2&limit=2"),
    )
    .await?;
    assert_eq!(
        ids_of(&by_offset),
        ids_of(&page2),
        "offset paging still works"
    );

    let invalid = client
        .get(format!("{inspect_url}/api/requests?cursor=not-a-cursor"))
        .send()
        .await?;
    assert_eq!(invalid.status().as_u16(), 400);

    shutdown.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    test_db.cleanup().await?;
    Ok(())
}

async fn send_batch(
    client: &reqwest::Client,
    proxy_url: &str,
    system_text: &str,
    count: usize,
) -> anyhow::Result<()> {
    for _ in 0..count {
        let session = uuid::Uuid::new_v4().to_string();
        send_request(client, proxy_url, &session, system_text, "u").await?;
    }
    Ok(())
}

fn distinct_prompt_shas(rows: &[Value]) -> Vec<String> {
    let mut shas: Vec<String> = rows
        .iter()
        .filter_map(|r| {
            r.get("prompt_sha256")
                .and_then(|v| v.as_str().map(str::to_string))
        })
        .collect();
    shas.sort();
    shas.dedup();
    shas
}

fn assert_rows_have_sha(rows: &[Value], expected_sha: &str) {
    for row in rows {
        assert_eq!(
            row.get("prompt_sha256").and_then(|v| v.as_str()),
            Some(expected_sha),
            "filter on {expected_sha} returned a row with a different prompt_sha256"
        );
    }
}

#[tokio::test]
async fn list_requests_filters_by_prompt_sha256() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    mount_ok_response(&mock_server).await;

    let Some(test_db_factory) = db_helpers::TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;
    let (proxy_url, inspect_url, shutdown) =
        db_helpers::start_proxy_with_db_at_composed(&mock_server.uri(), &test_db.database_url)
            .await?;

    let client = reqwest::Client::new();

    send_batch(&client, &proxy_url, "shape-A", 3).await?;
    send_batch(&client, &proxy_url, "shape-B", 2).await?;

    let unfiltered =
        get_requests_json(&client, &format!("{inspect_url}/api/requests?limit=200")).await?;
    let shas = distinct_prompt_shas(&unfiltered);
    anyhow::ensure!(
        shas.len() == 2,
        "expected exactly 2 distinct shas, got {} (shas: {:?})",
        shas.len(),
        shas
    );
    let sha_a = &shas[0];
    let sha_b = &shas[1];

    let filtered_a = get_requests_json(
        &client,
        &format!("{inspect_url}/api/requests?prompt_sha256={sha_a}&limit=200"),
    )
    .await?;
    assert!(
        !filtered_a.is_empty(),
        "sha_a filter must return at least one row"
    );
    assert_rows_have_sha(&filtered_a, sha_a);

    let filtered_b = get_requests_json(
        &client,
        &format!("{inspect_url}/api/requests?prompt_sha256={sha_b}&limit=200"),
    )
    .await?;
    assert_rows_have_sha(&filtered_b, sha_b);

    let empty_filter = get_requests_json(
        &client,
        &format!("{inspect_url}/api/requests?prompt_sha256=&limit=200"),
    )
    .await?;
    assert_eq!(
        empty_filter.len(),
        unfiltered.len(),
        "empty `?prompt_sha256=` must behave as filter absent"
    );

    let unknown_resp = client
        .get(format!(
            "{inspect_url}/api/requests?prompt_sha256={}",
            "0000000000000000000000000000000000000000000000000000000000000000"
        ))
        .send()
        .await?;
    assert_eq!(unknown_resp.status(), 200);
    let unknown_body: Vec<Value> = unknown_resp.json().await?;
    assert!(
        unknown_body.is_empty(),
        "drilldown for unknown sha must return [] not 404"
    );

    shutdown.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn list_requests_orders_by_created_at_desc() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    mount_ok_response(&mock_server).await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, _pool, test_db) = proxy.into_parts();

    let session = uuid::Uuid::new_v4().to_string();
    let client = reqwest::Client::new();

    for i in 0..4 {
        send_request(&client, &proxy_url, &session, &format!("ordering-{i}"), "u").await?;
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    let arr = get_requests_json(&client, &format!("{inspect_url}/api/requests?limit=200")).await?;
    assert!(
        arr.len() >= 4,
        "must have at least 4 rows, got {}",
        arr.len()
    );

    for window in arr.windows(2) {
        let a = window[0]
            .get("created_at")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing created_at on row a"))?;
        let b = window[1]
            .get("created_at")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing created_at on row b"))?;
        assert!(a >= b, "DESC ordering broken: {a} should be >= {b}",);
    }

    shutdown.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    test_db.cleanup().await?;
    Ok(())
}

fn extract_ids(v: &[Value]) -> anyhow::Result<Vec<u64>> {
    v.iter()
        .map(|r| {
            r.get("id")
                .and_then(Value::as_u64)
                .ok_or_else(|| anyhow::anyhow!("missing id"))
        })
        .collect()
}

#[tokio::test]
async fn list_requests_order_asc_is_exact_mirror_of_default_desc() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    mount_ok_response(&mock_server).await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, _pool, test_db) = proxy.into_parts();

    let session = uuid::Uuid::new_v4().to_string();
    let client = reqwest::Client::new();
    for i in 0..3 {
        send_request(&client, &proxy_url, &session, &format!("order-{i}"), "u").await?;
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    let default_page =
        get_requests_json(&client, &format!("{inspect_url}/api/requests?limit=200")).await?;
    let desc_page = get_requests_json(
        &client,
        &format!("{inspect_url}/api/requests?limit=200&order=desc"),
    )
    .await?;
    let default_ids = extract_ids(&default_page)?;
    let desc_ids = extract_ids(&desc_page)?;
    assert_eq!(
        default_ids, desc_ids,
        "explicit order=desc must equal the default ordering"
    );

    let asc_page = get_requests_json(
        &client,
        &format!("{inspect_url}/api/requests?limit=200&order=asc"),
    )
    .await?;
    let mut asc_ids = extract_ids(&asc_page)?;
    asc_ids.reverse();
    assert_eq!(
        asc_ids, desc_ids,
        "order=asc must be the exact reverse of order=desc"
    );

    let bad = client
        .get(format!("{}/api/requests?order=sideways", &inspect_url))
        .send()
        .await?;
    assert_eq!(
        bad.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "invalid order value must be rejected with 400"
    );

    shutdown.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    test_db.cleanup().await?;
    Ok(())
}
