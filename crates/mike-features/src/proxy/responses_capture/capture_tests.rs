use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::relay_test_support::{
    REQUEST, STREAM, body, captured, harness, next_recorded, sse_upstream,
};
use super::routes;
use crate::store::persistence::runtime_repo::RuntimePersist as _;

#[tokio::test]
async fn zstd_wire_is_unchanged_and_complete_decoded_context_reuses_chunks() -> anyhow::Result<()> {
    let encoded = include_bytes!("../../../../mike-codex/src/capture/fixtures/large.zst");
    let decoded: Vec<_> = (0..256 * 4097).map(|index| index as u8).collect();
    let server = sse_upstream("/v1/responses").await;
    let mut h = harness(&format!("{}/v1", server.uri()), "http://127.0.0.1:1")
        .await?
        .ok_or_else(|| anyhow::anyhow!("capture test requires the test database"))?;
    h.state
        .toggles
        .store(std::sync::Arc::new(Default::default()));
    let thread = Uuid::new_v4();
    let mut documents = Vec::new();
    for _ in 0..2 {
        let response = routes(h.state.clone())
            .oneshot(
                Request::post("/codex/responses")
                    .header("thread-id", thread.to_string())
                    .header("content-encoding", "zstd")
                    .body(Body::from(encoded.as_slice()))?,
            )
            .await?;
        assert_eq!(
            response.into_body().collect().await?.to_bytes().as_ref(),
            STREAM
        );
        let id = next_recorded(&mut h).await?;
        let capture = captured(&h, id).await?;
        assert_eq!(capture.metadata["request_body_encoding"], "zstd");
        assert_eq!(capture.metadata["request_stored_encoding"], "identity");
        assert_eq!(body(&h, capture.request_document_id).await?, decoded);
        assert_eq!(capture.request_document_id, capture.sent_document_id);
        documents.push(capture.request_document_id);
    }
    assert_ne!(documents[0], documents[1]);
    let first = h
        .persist
        .runtime()
        .capture_document_chunks(documents[0], 0, 200)
        .await?;
    let second = h
        .persist
        .runtime()
        .capture_document_chunks(documents[1], 0, 200)
        .await?;
    assert_eq!(first.len(), second.len());
    assert!(
        second
            .iter()
            .all(|chunk| first.iter().any(|old| old.id == chunk.id))
    );
    for request in server.received_requests().await.unwrap_or_default() {
        assert_eq!(request.body, encoded);
        assert_eq!(
            request
                .headers
                .get("content-encoding")
                .and_then(|value| value.to_str().ok()),
            Some("zstd")
        );
    }
    Ok(())
}

#[tokio::test]
async fn unknown_oversized_response_frames_remain_fully_captured() -> anyhow::Result<()> {
    let mut response_body = b"event: future.event\ndata: ".to_vec();
    response_body.extend(vec![b'x'; 1024 * 1024 + 1]);
    response_body.extend_from_slice(b"\n\nunknown binary\0\xff\n\n");
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_bytes(response_body.clone()),
        )
        .mount(&server)
        .await;
    let mut h = harness(&format!("{}/v1", server.uri()), "http://127.0.0.1:1")
        .await?
        .ok_or_else(|| anyhow::anyhow!("capture test requires the test database"))?;
    let response = routes(h.state.clone())
        .oneshot(Request::post("/codex/responses").body(Body::from(REQUEST))?)
        .await?;
    assert_eq!(
        response.into_body().collect().await?.to_bytes(),
        response_body
    );
    let id = next_recorded(&mut h).await?;
    let capture = captured(&h, id).await?;
    let response = capture
        .response_document_id
        .ok_or_else(|| anyhow::anyhow!("missing response"))?;
    assert_eq!(body(&h, response).await?, response_body);
    assert!(
        capture.metadata["projection_gaps"]
            .as_u64()
            .is_some_and(|gaps| gaps > 0)
    );
    assert!(
        h.persist
            .runtime()
            .capture_document(response)
            .await?
            .is_some_and(|document| document.complete)
    );
    Ok(())
}

#[tokio::test]
async fn interrupted_staging_recovers_complete_received_bytes_under_the_same_session()
-> anyhow::Result<()> {
    use super::spool::{Manifest, Spool};
    use crate::store::persistence::runtime::capture::CaptureExchangeInput;
    let h = harness("http://127.0.0.1:1", "http://127.0.0.1:1")
        .await?
        .ok_or_else(|| anyhow::anyhow!("capture test requires the test database"))?;
    let root = h
        .state
        .capture_root
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("capture directory missing"))?;
    let spool = Spool::create(root).await?;
    let session = Uuid::new_v4();
    h.persist.ensure_runtime_session(session).await?;
    let manifest = Manifest {
        identity: Default::default(),
        exchange: CaptureExchangeInput {
            id: spool.id,
            session_uuid: Some(session),
            provider_thread_id: Some(Uuid::new_v4().to_string()),
            created_at: chrono::Utc::now(),
            metadata: json!({"response_status":200,
                "response_headers":[["content-type","text/event-stream"]],"unknown_field":{"future":true}}),
            request_document_id: 0,
            sent_document_id: 0,
            response_document_id: None,
            status: "pending".into(),
        },
        request_predecessor: None,
        request_complete: true,
        response_complete: false,
        response_received: true,
    };
    tokio::fs::write(spool.directory.join("request"), REQUEST).await?;
    let partial = b"event: future.partial\ndata: {\"text\":\"kept entirely\"}\n\n";
    tokio::fs::write(spool.directory.join("response"), partial).await?;
    spool
        .write_manifest(&manifest)
        .await
        .map_err(anyhow::Error::msg)?;
    super::recovery::recover(h.state.clone()).await;
    assert!(
        h.persist
            .runtime()
            .get_capture_exchange(spool.id)
            .await?
            .is_none()
    );
    let id = spool.id;
    let directory = spool.directory.clone();
    drop(spool);
    super::recovery::recover(h.state.clone()).await;
    let capture = h
        .persist
        .runtime()
        .get_capture_exchange(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("capture was not recovered"))?;
    assert_eq!(capture.status, "aborted");
    assert_eq!(capture.metadata["response_status"], 200);
    assert_eq!(capture.metadata["unknown_field"], json!({"future":true}));
    assert_eq!(capture.session_uuid, Some(session));
    assert_eq!(body(&h, capture.request_document_id).await?, REQUEST);
    let response = capture
        .response_document_id
        .ok_or_else(|| anyhow::anyhow!("missing partial response"))?;
    assert_eq!(body(&h, response).await?, partial);
    assert!(
        h.persist
            .runtime()
            .capture_document(response)
            .await?
            .is_some_and(|row| !row.complete)
    );
    assert!(
        h.persist
            .runtime()
            .list_capture_exchanges(session, 20, 0)
            .await?
            .iter()
            .any(|row| row.id == id)
    );
    assert!(!directory.exists());
    super::recovery::recover(h.state.clone()).await;
    Ok(())
}

#[tokio::test]
async fn recovery_retries_proxy_identity_after_an_outage_without_hooks() -> anyhow::Result<()> {
    use super::spool::{IdentityInput, Manifest, Spool};
    use crate::store::persistence::runtime::capture::CaptureExchangeInput;
    let h = harness("http://127.0.0.1:1", "http://127.0.0.1:1")
        .await?
        .ok_or_else(|| anyhow::anyhow!("capture test requires the test database"))?;
    let root = h
        .state
        .capture_root
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("capture directory missing"))?;
    let spool = Spool::create(root).await?;
    let launch = Uuid::new_v4();
    let thread = Uuid::new_v4().to_string();
    let home = h.home.path().canonicalize()?;
    sqlx::query("INSERT INTO launch.launches (session_uuid, backend, folder, identity) VALUES ($1, 'herdr', $2, $3)")
        .bind(launch).bind(home.to_string_lossy().as_ref())
        .bind(json!({"runtime":"codex","runtime_home":home})).execute(h.persist.pool()).await?;
    let mut manifest = Manifest {
        identity: IdentityInput {
            launch: Some(launch),
            home: Some(home),
            thread: Some(thread.clone()),
            cwd: None,
        },
        exchange: CaptureExchangeInput {
            id: spool.id,
            session_uuid: None,
            provider_thread_id: Some(thread),
            created_at: chrono::Utc::now(),
            metadata: json!({"response_status":200}),
            request_document_id: 0,
            sent_document_id: 0,
            response_document_id: None,
            status: "pending".into(),
        },
        request_predecessor: None,
        request_complete: true,
        response_complete: false,
        response_received: true,
    };
    tokio::fs::write(spool.directory.join("request"), REQUEST).await?;
    tokio::fs::write(spool.directory.join("response"), b"partial captured reply").await?;
    spool
        .write_manifest(&manifest)
        .await
        .map_err(anyhow::Error::msg)?;
    let refused = super::capture_write::persist(&h.persist, &spool.directory, &mut manifest).await;
    assert!(refused.is_err_and(|error| error.contains("proxy session identity")));
    assert!(
        h.persist
            .runtime()
            .get_capture_exchange(spool.id)
            .await?
            .is_none()
    );
    let id = spool.id;
    drop(spool);
    super::recovery::recover(h.state.clone()).await;
    let captured = h
        .persist
        .runtime()
        .get_capture_exchange(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("capture was not recovered"))?;
    assert_eq!(captured.session_uuid, Some(launch));
    assert_eq!(
        captured.metadata["correlation"]["normalized_session"]["source"],
        "recovered_proxy_identity"
    );
    assert!(
        h.persist
            .runtime()
            .list_capture_exchanges(launch, 20, 0)
            .await?
            .iter()
            .any(|row| row.id == id)
    );
    let registered: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workspace.session_registrations r JOIN sessions.sessions s ON s.id=r.session_id WHERE s.session_uuid=$1)")
        .bind(launch).fetch_one(h.persist.pool()).await?;
    assert!(registered);
    Ok(())
}
