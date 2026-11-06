use crate::common;

#[tokio::test]
async fn list_sessions_rejects_one_char_search_with_400() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let (_proxy_url, inspect_url) = common::start_proxy(&mock.uri()).await?;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{inspect_url}/api/sessions?search=a"))
        .send()
        .await?;
    assert_eq!(
        resp.status().as_u16(),
        400,
        "single-char search must return 400"
    );
    let body = resp.text().await?;
    assert!(
        body.contains("at least 2 characters"),
        "400 body must surface the gate reason; got {body:?}"
    );
    Ok(())
}

#[tokio::test]
async fn list_requests_rejects_a_malformed_cursor_with_400() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let (_proxy_url, inspect_url) = common::start_proxy(&mock.uri()).await?;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{inspect_url}/api/requests?cursor=not-a-cursor"))
        .send()
        .await?;
    assert_eq!(
        resp.status().as_u16(),
        400,
        "a cursor that does not decode is a client error"
    );
    let body = resp.text().await?;
    assert!(body.contains("invalid cursor"), "{body:?}");

    let resp = client
        .get(format!("{inspect_url}/api/requests?limit=5"))
        .send()
        .await?;
    assert_eq!(resp.status().as_u16(), 200);
    assert!(
        resp.headers().get("x-next-cursor").is_none(),
        "an empty store has no next page"
    );
    Ok(())
}

#[tokio::test]
async fn list_sessions_accepts_two_char_search_with_200() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let (_proxy_url, inspect_url) = common::start_proxy(&mock.uri()).await?;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{inspect_url}/api/sessions?search=ab"))
        .send()
        .await?;
    assert_eq!(
        resp.status().as_u16(),
        200,
        "two-char search must return 200"
    );
    Ok(())
}

#[tokio::test]
async fn list_sessions_accepts_three_char_search_with_200() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let (_proxy_url, inspect_url) = common::start_proxy(&mock.uri()).await?;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{inspect_url}/api/sessions?search=abc"))
        .send()
        .await?;
    assert_eq!(resp.status().as_u16(), 200);
    Ok(())
}

#[tokio::test]
async fn list_sessions_accepts_absent_search_with_200() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let (_proxy_url, inspect_url) = common::start_proxy(&mock.uri()).await?;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{inspect_url}/api/sessions"))
        .send()
        .await?;
    assert_eq!(
        resp.status().as_u16(),
        200,
        "absent search must return 200 (filter optional)"
    );
    Ok(())
}

#[tokio::test]
async fn list_requests_rejects_non_hex_prompt_sha256_with_400() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let (_proxy_url, inspect_url) = common::start_proxy(&mock.uri()).await?;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{inspect_url}/api/requests?prompt_sha256=NOT_HEX"))
        .send()
        .await?;
    assert_eq!(
        resp.status().as_u16(),
        400,
        "non-hex prompt_sha256 must return 400"
    );
    let body = resp.text().await?;
    assert!(
        body.contains("64 lowercase hex"),
        "400 body must surface the validation rule; got {body:?}"
    );
    Ok(())
}

#[tokio::test]
async fn list_requests_rejects_short_prompt_sha256_with_400() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let (_proxy_url, inspect_url) = common::start_proxy(&mock.uri()).await?;
    let client = reqwest::Client::new();

    let short = "0".repeat(32);
    let resp = client
        .get(format!("{inspect_url}/api/requests?prompt_sha256={short}"))
        .send()
        .await?;
    assert_eq!(
        resp.status().as_u16(),
        400,
        "short prompt_sha256 must return 400"
    );
    Ok(())
}

#[tokio::test]
async fn list_requests_rejects_uppercase_prompt_sha256_with_400() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let (_proxy_url, inspect_url) = common::start_proxy(&mock.uri()).await?;
    let client = reqwest::Client::new();

    let upper = "A".repeat(64);
    let resp = client
        .get(format!("{inspect_url}/api/requests?prompt_sha256={upper}"))
        .send()
        .await?;
    assert_eq!(
        resp.status().as_u16(),
        400,
        "uppercase prompt_sha256 must return 400"
    );
    Ok(())
}

#[tokio::test]
async fn list_requests_accepts_valid_prompt_sha256_with_200() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let (_proxy_url, inspect_url) = common::start_proxy(&mock.uri()).await?;
    let client = reqwest::Client::new();

    let valid = "abcdef0123456789".repeat(4);
    assert_eq!(valid.len(), 64);
    let resp = client
        .get(format!("{inspect_url}/api/requests?prompt_sha256={valid}"))
        .send()
        .await?;
    assert_eq!(
        resp.status().as_u16(),
        200,
        "valid prompt_sha256 must return 200"
    );
    Ok(())
}

#[tokio::test]
async fn list_requests_accepts_absent_prompt_sha256_with_200() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let (_proxy_url, inspect_url) = common::start_proxy(&mock.uri()).await?;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{inspect_url}/api/requests"))
        .send()
        .await?;
    assert_eq!(
        resp.status().as_u16(),
        200,
        "absent prompt_sha256 must return 200 (filter optional)"
    );
    Ok(())
}
