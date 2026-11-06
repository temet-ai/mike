use std::sync::Arc;

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::test_support::{TestVerdict, test_schema, valid_response};
use super::*;

#[test]
fn client_construction_succeeds() {
    let client = OpenRouterClient::new(
        "sk-test".to_string(),
        "google/gemini-3.5-flash-lite".to_string(),
        None,
        None,
    );
    assert!(client.is_ok());
}

#[test]
fn client_clone_is_cheap() -> anyhow::Result<()> {
    let client = OpenRouterClient::new(
        "sk-test".to_string(),
        "google/gemini-3.5-flash-lite".to_string(),
        None,
        None,
    )?;
    let _clone = client.clone();
    assert!(Arc::ptr_eq(&client.inner, &_clone.inner));
    Ok(())
}

#[tokio::test]
async fn with_api_key_swaps_key_sends_new_bearer_token() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(wiremock::matchers::header(
            "authorization",
            "Bearer new-key",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(valid_response(true)))
        .expect(1)
        .mount(&server)
        .await;

    let base = OpenRouterClient::new(
        "old-key".to_string(),
        "google/gemini-3.5-flash-lite".to_string(),
        Some(server.uri()),
        None,
    )?;
    let rekeyed = base.with_api_key("new-key".to_string())?;

    let result = rekeyed
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await?;
    assert_eq!(result, TestVerdict { label: true });

    server.verify().await;
    Ok(())
}
