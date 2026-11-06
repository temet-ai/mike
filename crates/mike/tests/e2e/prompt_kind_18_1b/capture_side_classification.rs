use crate::anthropic_fixtures;
use crate::db_helpers;

use anthropic_fixtures::RequestBody;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::support::{minimal_sse_response, wait_for_prompt_kind_classification};

#[tokio::test]
async fn capture_side_write_classifies_new_operator_turn() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(minimal_sse_response().as_bytes(), "text/event-stream"),
        )
        .mount(&mock)
        .await;

    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&mock.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (proxy_url, _inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "1a000000-0000-4000-8000-000000000001";
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("x-claude-code-session-id", session_id)
        .header("content-type", "application/json")
        .json(
            &RequestBody::default()
                .content("implement the new feature please")
                .stream(true)
                .build(),
        )
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let _ = resp.bytes().await?;

    let kind_code = wait_for_prompt_kind_classification(&pool, session_id).await?;
    assert_eq!(
        kind_code, "operator",
        "real operator prompt must classify as 'operator'"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn capture_side_write_classifies_agent_message_wrapper_as_teammate_message()
-> anyhow::Result<()> {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(minimal_sse_response().as_bytes(), "text/event-stream"),
        )
        .mount(&mock)
        .await;

    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&mock.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (proxy_url, _inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "1a000000-0000-4000-8000-000000000003";
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("x-claude-code-session-id", session_id)
        .header("content-type", "application/json")
        .json(&RequestBody::default()
            .content("Another Claude session sent a message: \
                <agent-message from=\"rust-developer\">the migration is ready for review</agent-message>")
            .stream(true)
            .build())
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let _ = resp.bytes().await?;

    let kind_code = wait_for_prompt_kind_classification(&pool, session_id).await?;
    assert_eq!(
        kind_code, "teammate-message",
        "an <agent-message from=...> wrapper must classify as 'teammate-message', not 'operator'"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
