use crate::anthropic_fixtures;
use crate::db_cap_helpers;

use anthropic_fixtures::{MockResponse, RequestBody};
use db_cap_helpers::{TestDb, TestDbHandle};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn send_request(
    client: &reqwest::Client,
    proxy_url: &str,
    session_id: &str,
    user_text: &str,
) -> anyhow::Result<()> {
    let body = RequestBody::default()
        .max_tokens(4)
        .content(user_text)
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
        "expected 200 OK from proxy, got {}",
        response.status()
    );
    let _ = response.bytes().await?;
    Ok(())
}

struct BootedProxy {
    proxy_url: String,
    inspect_url: String,
    shutdown: tokio_util::sync::CancellationToken,
    test_db: TestDbHandle,
}

async fn boot_proxy(mock_uri: &str) -> anyhow::Result<Option<BootedProxy>> {
    let Some(test_db_factory) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let test_db = test_db_factory.acquire_isolated().await?;
    let (proxy_url, inspect_url, shutdown) =
        db_cap_helpers::start_proxy_with_db_at_capped(mock_uri, &test_db.database_url, 64).await?;
    Ok(Some(BootedProxy {
        proxy_url,
        inspect_url,
        shutdown,
        test_db,
    }))
}

#[tokio::test]
async fn get_session_etag_yields_304_then_200_after_new_request() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(MockResponse::new("msg_etag").usage(1, 1).build())
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let Some(booted) = boot_proxy(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };

    let session = uuid::Uuid::new_v4().to_string();
    let client = reqwest::Client::new();

    send_request(&client, &booted.proxy_url, &session, "etag-first").await?;
    let first = client
        .get(format!("{}/api/sessions/{session}", booted.inspect_url))
        .send()
        .await?;
    anyhow::ensure!(
        first.status() == 200,
        "first GET expected 200, got {}",
        first.status()
    );
    let etag = first
        .headers()
        .get(reqwest::header::ETAG)
        .ok_or_else(|| anyhow::anyhow!("first GET must carry an ETag"))?
        .to_str()?
        .to_string();

    let not_modified = client
        .get(format!("{}/api/sessions/{session}", booted.inspect_url))
        .header(reqwest::header::IF_NONE_MATCH, &etag)
        .send()
        .await?;
    anyhow::ensure!(
        not_modified.status() == 304,
        "conditional GET with matching ETag expected 304, got {}",
        not_modified.status()
    );
    let nm_body = not_modified.bytes().await?;
    anyhow::ensure!(
        nm_body.is_empty(),
        "304 response must have an empty body, got {} bytes",
        nm_body.len()
    );

    send_request(&client, &booted.proxy_url, &session, "etag-second").await?;
    let refreshed = client
        .get(format!("{}/api/sessions/{session}", booted.inspect_url))
        .header(reqwest::header::IF_NONE_MATCH, &etag)
        .send()
        .await?;
    anyhow::ensure!(
        refreshed.status() == 200,
        "conditional GET after a new request expected 200, got {}",
        refreshed.status()
    );
    let new_etag = refreshed
        .headers()
        .get(reqwest::header::ETAG)
        .ok_or_else(|| anyhow::anyhow!("refreshed GET must carry an ETag"))?
        .to_str()?
        .to_string();
    anyhow::ensure!(
        new_etag != etag,
        "ETag must change after a new request landed; both were {etag}"
    );

    booted.shutdown.cancel();
    booted.test_db.cleanup().await?;
    Ok(())
}
