use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::test_support::{
    TestVerdict, client_for, client_with_limiter, response_with_usage, test_schema, valid_response,
};
use super::*;
use crate::llm::provider::local::{LOCAL_BUSY, install_endpoint_for_tests};

const SET_KEY_ENV: &str = "PATH";
const UNSET_KEY_ENV: &str = "MIKE_TEST_LOCAL_CALL_KEY_NEVER_SET";

struct EndpointGuard;

impl EndpointGuard {
    fn point_at(server: &MockServer, key_env: &str) -> Self {
        install_endpoint_for_tests(&server.uri(), key_env);
        Self
    }
}

impl Drop for EndpointGuard {
    fn drop(&mut self) {
        install_endpoint_for_tests("", UNSET_KEY_ENV);
    }
}

async fn answering_server(body: Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;
    server
}

async fn mount_slots(server: &MockServer, processing: &[bool]) {
    let slots: Vec<Value> = processing
        .iter()
        .map(|p| json!({"is_processing": p}))
        .collect();
    Mock::given(method("GET"))
        .and(path("/slots"))
        .respond_with(ResponseTemplate::new(200).set_body_json(slots))
        .mount(server)
        .await;
}

async fn posts(server: &MockServer) -> Vec<wiremock::Request> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.method == wiremock::http::Method::POST)
        .collect()
}

fn local_client(server: &MockServer) -> anyhow::Result<OpenRouterClient> {
    Ok(client_for(server)?.with_model("local/gemma-4-12b".to_owned())?)
}

#[tokio::test]
#[serial_test::serial(llm_provider)]
async fn attempt_verdict_local_sends_the_no_thinking_body_without_openrouter_fields()
-> anyhow::Result<()> {
    let server = answering_server(valid_response(true)).await;
    let _endpoint = EndpointGuard::point_at(&server, SET_KEY_ENV);
    let client = local_client(&server)?;

    let (verdict, usage): (TestVerdict, _) = client
        .attempt_verdict("sys", "user", &test_schema())
        .await?;

    assert_eq!(verdict, TestVerdict { label: true });
    assert_eq!(usage.cost_usd, Some(0.0));
    let requests = posts(&server).await;
    let [request] = requests.as_slice() else {
        anyhow::bail!("expected one request, saw {}", requests.len());
    };
    let body: Value = serde_json::from_slice(&request.body)?;
    assert_eq!(body["model"], "gemma-4-12b");
    assert_eq!(body["chat_template_kwargs"]["enable_thinking"], false);
    assert_eq!(body["response_format"]["type"], "json_schema");
    assert_eq!(body["response_format"]["json_schema"]["strict"], true);
    assert!(body.get("provider").is_none());
    assert!(body.get("reasoning").is_none());
    let expected = format!("Bearer {}", std::env::var(SET_KEY_ENV)?);
    let auth = request
        .headers
        .get("authorization")
        .map(|v| v.to_str().unwrap_or_default().to_owned());
    assert_eq!(auth.as_deref(), Some(expected.as_str()));
    Ok(())
}

#[tokio::test]
#[serial_test::serial(llm_provider)]
async fn attempt_verdict_local_without_a_key_sends_no_authorization_header() -> anyhow::Result<()> {
    let server = answering_server(valid_response(false)).await;
    let _endpoint = EndpointGuard::point_at(&server, UNSET_KEY_ENV);
    let client = local_client(&server)?;

    let (verdict, _): (TestVerdict, _) = client
        .attempt_verdict("sys", "user", &test_schema())
        .await?;

    assert_eq!(verdict, TestVerdict { label: false });
    let requests = server.received_requests().await.unwrap_or_default();
    assert!(
        requests
            .iter()
            .all(|r| r.headers.get("authorization").is_none())
    );
    Ok(())
}

#[tokio::test]
#[serial_test::serial(llm_provider)]
async fn attempt_verdict_local_length_finish_reason_is_output_truncated() -> anyhow::Result<()> {
    let server = answering_server(response_with_usage(true, Some("length"))).await;
    let _endpoint = EndpointGuard::point_at(&server, UNSET_KEY_ENV);
    let client = local_client(&server)?;

    let result = client
        .attempt_verdict::<TestVerdict>("sys", "user", &test_schema())
        .await;

    assert!(matches!(result, Err(LlmError::OutputTruncated { .. })));
    Ok(())
}

#[tokio::test]
#[serial_test::serial(llm_provider)]
async fn attempt_verdict_local_http_error_maps_to_http_status() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let _endpoint = EndpointGuard::point_at(&server, UNSET_KEY_ENV);
    let client = local_client(&server)?;

    let result = client
        .attempt_verdict::<TestVerdict>("sys", "user", &test_schema())
        .await;

    assert!(matches!(result, Err(LlmError::Http { status: 503 })));
    Ok(())
}

#[tokio::test]
#[serial_test::serial(llm_provider)]
async fn attempt_verdict_local_with_no_url_is_provider_unavailable_and_names_the_setting()
-> anyhow::Result<()> {
    let server = MockServer::start().await;
    install_endpoint_for_tests("", UNSET_KEY_ENV);
    let client = local_client(&server)?;

    let result = client
        .attempt_verdict::<TestVerdict>("sys", "user", &test_schema())
        .await;

    match result {
        Err(LlmError::ProviderUnavailable { missing, .. }) => {
            assert!(missing.contains("llm.local.url"), "{missing}");
        }
        other => anyhow::bail!("expected ProviderUnavailable, got {other:?}"),
    }
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
#[serial_test::serial(llm_provider)]
async fn attempt_verdict_local_two_processing_slots_is_busy_and_sends_no_post() -> anyhow::Result<()>
{
    let server = answering_server(valid_response(true)).await;
    mount_slots(&server, &[true, true]).await;
    let _endpoint = EndpointGuard::point_at(&server, UNSET_KEY_ENV);
    let client = local_client(&server)?;

    let result = client
        .attempt_verdict::<TestVerdict>("sys", "user", &test_schema())
        .await;

    match result {
        Err(LlmError::ProviderUnavailable { missing, .. }) => assert_eq!(missing, LOCAL_BUSY),
        other => anyhow::bail!("expected a busy refusal, got {other:?}"),
    }
    assert!(posts(&server).await.is_empty());
    Ok(())
}

#[tokio::test]
#[serial_test::serial(llm_provider)]
async fn attempt_verdict_local_one_processing_slot_still_answers() -> anyhow::Result<()> {
    let server = answering_server(valid_response(true)).await;
    mount_slots(&server, &[true, false]).await;
    let _endpoint = EndpointGuard::point_at(&server, UNSET_KEY_ENV);
    let client = local_client(&server)?;

    let (verdict, _): (TestVerdict, _) = client
        .attempt_verdict("sys", "user", &test_schema())
        .await?;

    assert_eq!(verdict, TestVerdict { label: true });
    Ok(())
}

#[tokio::test]
#[serial_test::serial(llm_provider)]
async fn attempt_verdict_local_does_not_draw_on_the_shared_call_limiter() -> anyhow::Result<()> {
    let server = answering_server(valid_response(true)).await;
    let _endpoint = EndpointGuard::point_at(&server, UNSET_KEY_ENV);
    let exhausted = CallLimiter::new(0, Duration::from_secs(60), 0, Duration::from_secs(60));
    let client =
        client_with_limiter(&server, exhausted)?.with_model("local/gemma-4-12b".to_owned())?;

    let (verdict, _): (TestVerdict, _) = client
        .attempt_verdict("sys", "user", &test_schema())
        .await?;

    assert_eq!(verdict, TestVerdict { label: true });
    Ok(())
}
