use super::*;
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SESSION: &str = "00000000-0000-4000-8000-000000000001";

#[test]
fn process_thread_current_claude_does_not_inherit_parent_codex_identity() {
    assert_eq!(process_thread(Some(SESSION), Some("parent".into())), None);
    assert_eq!(
        process_thread(None, Some("current".into())),
        Some("current".into())
    );
    assert_eq!(
        process_thread(Some(" "), Some("current".into())),
        Some("current".into())
    );
}

#[test]
fn request_thread_metadata_accepts_both_codex_sources() -> anyhow::Result<()> {
    for value in [
        json!({"threadId": " opaque:thread "}),
        json!({
            "x-codex-turn-metadata": {"thread_id": " opaque:thread "}
        }),
    ] {
        let meta: RequestMetaObject = serde_json::from_value(value)?;
        assert_eq!(request_thread(Some(&meta))?, Some(" opaque:thread "));
    }
    assert_eq!(request_thread(None)?, None);
    Ok(())
}

#[test]
fn request_thread_invalid_or_conflicting_metadata_is_refused() -> anyhow::Result<()> {
    for value in [
        json!({"threadId": " "}),
        json!({"threadId": 7}),
        json!({"x-codex-turn-metadata": {}}),
        json!({"x-codex-turn-metadata": "invalid"}),
        json!({"x-codex-turn-metadata": {"thread_id": 7}}),
        json!({
            "threadId": "child", "x-codex-turn-metadata": {"thread_id": "parent"}
        }),
    ] {
        let meta: RequestMetaObject = serde_json::from_value(value)?;
        assert!(request_thread(Some(&meta)).is_err());
    }
    Ok(())
}

#[tokio::test]
async fn resolve_native_thread_uses_binding_and_preserves_opaque_id() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/codex/session"))
        .and(query_param("thread_id", " native:/?id & "))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"session_id": SESSION})))
        .expect(1)
        .mount(&mock)
        .await;
    let client = InspectClient::new(mock.uri());
    assert_eq!(
        resolve(&client, Some(" native:/?id & "), Some("parent")).await?,
        Some(SESSION.into())
    );
    Ok(())
}

#[tokio::test]
async fn resolve_unknown_or_ambiguous_native_thread_does_not_use_parent() -> anyhow::Result<()> {
    for status in [404, 409, 503] {
        let mock = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/codex/session"))
            .respond_with(ResponseTemplate::new(status))
            .mount(&mock)
            .await;
        let client = InspectClient::new(mock.uri());
        let error = resolve(&client, Some("child"), Some(SESSION))
            .await
            .err()
            .ok_or_else(|| anyhow::anyhow!("unresolved child used parent identity"))?;
        assert!(error.message.contains("child"));
    }
    Ok(())
}

#[tokio::test]
async fn resolve_invalid_binding_does_not_use_native_uuid_or_parent() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"session_id": "invalid"})))
        .mount(&mock)
        .await;
    assert!(
        resolve(
            &InspectClient::new(mock.uri()),
            Some(SESSION),
            Some(SESSION)
        )
        .await
        .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn resolve_without_native_thread_uses_valid_process_identity_without_lookup()
-> anyhow::Result<()> {
    let mock = MockServer::start().await;
    let client = InspectClient::new(mock.uri());
    assert_eq!(
        resolve(&client, None, Some(SESSION)).await?,
        Some(SESSION.into())
    );
    assert_eq!(resolve(&client, None, Some("invalid")).await?, None);
    assert_eq!(resolve(&client, None, None).await?, None);
    assert!(
        mock.received_requests()
            .await
            .ok_or_else(|| anyhow::anyhow!("requests unavailable"))?
            .is_empty()
    );
    Ok(())
}
