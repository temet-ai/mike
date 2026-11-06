use crate::anthropic_fixtures;
use crate::common;

use anthropic_fixtures::RequestBody;
use proptest::prelude::*;
use proptest::test_runner::TestCaseError;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn as_fail<E: std::fmt::Display>(ctx: &str) -> impl Fn(E) -> TestCaseError + '_ {
    move |e| TestCaseError::fail(format!("{ctx}: {e}"))
}

fn valid_header_value() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9,.;:/_\\-=+][a-zA-Z0-9 ,.;:/_\\-=+]{0,98}[a-zA-Z0-9,.;:/_\\-=+]"
}

fn custom_header_name() -> impl Strategy<Value = String> {
    (0..20u32).prop_map(|i| format!("x-custom-prop-{i}"))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn request_headers_survive_roundtrip(
        header_name in custom_header_name(),
        header_value in valid_header_value(),
    ) {
        let rt = tokio::runtime::Runtime::new().map_err(as_fail("tokio runtime"))?;
        rt.block_on(async {
            let mock_server = MockServer::start().await;

            Mock::given(method("POST"))
                .and(path("/v1/messages"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(json!({"ok": true}))
                        .insert_header("content-type", "application/json"),
                )
                .mount(&mock_server)
                .await;

            let (proxy_url, _inspect_url) = common::start_proxy(&mock_server.uri())
                .await
                .map_err(as_fail("start proxy"))?;
            let client = reqwest::Client::new();

            let resp = client
                .post(format!("{proxy_url}/v1/messages"))
                .header("x-api-key", "sk-test")
                .header("content-type", "application/json")
                .header(&header_name, &header_value)
                .json(&RequestBody::default().messages(json!([])).max_tokens(1).build())
                .send()
                .await
                .map_err(as_fail("send request"))?;

            prop_assert_eq!(resp.status().as_u16(), 200);

            let received = mock_server.received_requests().await;
            let received = received
                .as_ref()
                .ok_or_else(|| TestCaseError::fail("mock server has no request log"))?;
            prop_assert!(!received.is_empty());

            let upstream_req = &received[received.len() - 1];
            let upstream_val = upstream_req
                .headers
                .get(&header_name)
                .map(|v| v.to_str().unwrap_or("").to_string());

            prop_assert_eq!(
                upstream_val.as_deref(),
                Some(header_value.as_str()),
                "header {} must survive roundtrip",
                header_name
            );

            Ok(())
        })?;
    }

    #[test]
    fn response_body_bytes_unchanged(
        body_content in "[a-zA-Z0-9 ,.!?]{10,500}",
    ) {
        let rt = tokio::runtime::Runtime::new().map_err(as_fail("tokio runtime"))?;
        rt.block_on(async {
            let mock_server = MockServer::start().await;

            let response_body = json!({
                "id": "msg_prop",
                "type": "message",
                "content": body_content,
            });
            let expected_bytes = serde_json::to_vec(&response_body).map_err(as_fail("serialize"))?;

            Mock::given(method("POST"))
                .and(path("/v1/messages"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_bytes(expected_bytes.clone())
                        .insert_header("content-type", "application/json"),
                )
                .mount(&mock_server)
                .await;

            let (proxy_url, _inspect_url) = common::start_proxy(&mock_server.uri())
                .await
                .map_err(as_fail("start proxy"))?;
            let client = reqwest::Client::new();

            let resp = client
                .post(format!("{proxy_url}/v1/messages"))
                .header("x-api-key", "sk-test")
                .header("content-type", "application/json")
                .json(&RequestBody::default().messages(json!([])).max_tokens(1).build())
                .send()
                .await
                .map_err(as_fail("send request"))?;

            let actual_bytes = resp.bytes().await.map_err(as_fail("read body"))?;

            prop_assert_eq!(
                actual_bytes.as_ref(),
                expected_bytes.as_slice(),
                "response body must be byte-identical"
            );

            Ok(())
        })?;
    }
}
