use chrono::{Duration, Utc};
use serde_json::json;
use uuid::Uuid;

use super::{CaptureChunkInput, CaptureExchangeInput};
use crate::store::persistence::runtime::tests_observation_support::{Fixture, fixture};
use crate::store::persistence::runtime_repo::RuntimePersist as _;

#[path = "response_tests.rs"]
mod response_tests;

async fn setup() -> anyhow::Result<Fixture> {
    fixture()
        .await?
        .ok_or_else(|| anyhow::anyhow!("capture tests require a disposable database"))
}

fn exchange(f: &Fixture, document: i64) -> CaptureExchangeInput {
    CaptureExchangeInput {
        id: Uuid::new_v4(),
        session_uuid: Some(f.session),
        provider_thread_id: Some("test-thread".into()),
        created_at: Utc::now(),
        metadata: json!({"unknown":{"wire":"preserved"}}),
        request_document_id: document,
        sent_document_id: document,
        response_document_id: None,
        status: "complete".into(),
    }
}

#[tokio::test]
async fn capture_reused_chunks_reconstruct_complete_bytes_without_payload_copies()
-> anyhow::Result<()> {
    let f = setup().await?;
    let first = f.handle.runtime().create_capture_document(None).await?;
    let chunks = [
        CaptureChunkInput::New(b"{ \"input\": [".to_vec()),
        CaptureChunkInput::New(vec![0, 255, 128]),
        CaptureChunkInput::New(b"] }\n".to_vec()),
    ];
    let ids = f
        .handle
        .runtime()
        .append_capture_chunks(first, 0, &chunks)
        .await?;
    assert_eq!(
        f.handle
            .runtime()
            .append_capture_chunks(first, 0, &chunks)
            .await?,
        ids
    );
    f.handle
        .runtime()
        .finish_capture_document(first, true)
        .await?;
    let first_exchange = exchange(&f, first);
    assert_capture_activity(&f, &first_exchange).await?;
    assert_eq!(
        f.handle
            .runtime()
            .previous_capture_request(Some(f.session), Some("test-thread"))
            .await?,
        Some(first)
    );
    let second = f
        .handle
        .runtime()
        .create_capture_document(Some(first))
        .await?;
    let large = vec![7; 300 * 1024];
    let reused = [
        CaptureChunkInput::Existing(ids[0]),
        CaptureChunkInput::Existing(ids[1]),
        CaptureChunkInput::New(large.clone()),
        CaptureChunkInput::Existing(ids[2]),
    ];
    let second_ids = f
        .handle
        .runtime()
        .append_capture_chunks(second, 0, &reused)
        .await?;
    assert_eq!(&second_ids[..2], &ids[..2]);
    f.handle
        .runtime()
        .finish_capture_document(second, true)
        .await?;
    let second_exchange = exchange(&f, second);
    f.handle
        .runtime()
        .upsert_capture_exchange(&second_exchange)
        .await?;
    assert_document(&f, second, &large).await?;
    let page = f
        .handle
        .runtime()
        .list_capture_exchanges(f.session, 10, 0)
        .await?;
    assert_eq!(page.len(), 2);
    assert_eq!(page[0].metadata, second_exchange.metadata);
    assert_eq!(page[0].request_document_id, page[0].sent_document_id);
    assert!(
        f.handle
            .runtime()
            .list_capture_exchanges(Uuid::new_v4(), 10, 0)
            .await?
            .is_empty()
    );
    assert_empty_request_keeps_predecessor(&f, second).await?;
    f.cleanup().await
}

async fn assert_empty_request_keeps_predecessor(f: &Fixture, useful: i64) -> anyhow::Result<()> {
    let empty = f.handle.runtime().create_capture_document(None).await?;
    f.handle
        .runtime()
        .finish_capture_document(empty, true)
        .await?;
    let mut probe = exchange(f, empty);
    probe.metadata = json!({"path":"/models"});
    f.handle.runtime().upsert_capture_exchange(&probe).await?;
    assert_eq!(
        f.handle
            .runtime()
            .previous_capture_request(Some(f.session), Some("test-thread"))
            .await?,
        Some(useful)
    );
    assert_eq!(
        f.handle
            .runtime()
            .previous_capture_request(None, Some("test-thread"))
            .await?,
        Some(useful)
    );
    Ok(())
}

async fn assert_document(f: &Fixture, second: i64, large: &[u8]) -> anyhow::Result<()> {
    let rows = f
        .handle
        .runtime()
        .capture_document_chunks(second, 0, 2)
        .await?;
    let rest = f
        .handle
        .runtime()
        .capture_document_chunks(second, 2, 2)
        .await?;
    let body: Vec<u8> = rows
        .into_iter()
        .chain(rest)
        .flat_map(|chunk| chunk.body)
        .collect();
    let expected = [
        b"{ \"input\": [".as_slice(),
        &[0, 255, 128],
        large,
        b"] }\n",
    ]
    .concat();
    assert_eq!(body, expected);
    let info = f
        .handle
        .runtime()
        .capture_document(second)
        .await?
        .ok_or_else(|| anyhow::anyhow!("missing document"))?;
    assert_eq!(info.byte_size, body.len() as i64);
    assert_eq!(info.chunk_count, 4);
    assert!(info.complete && info.finished);
    let count = sqlx::query_scalar!("SELECT count(*) AS \"count!\" FROM runtime.capture_chunks")
        .fetch_one(f.handle.pool())
        .await?;
    assert_eq!(count, 4);
    Ok(())
}

async fn assert_capture_activity(f: &Fixture, input: &CaptureExchangeInput) -> anyhow::Result<()> {
    let before = session_activity(f).await?;
    f.handle.runtime().upsert_capture_exchange(input).await?;
    let inserted = session_activity(f).await?;
    assert!(inserted > before);
    f.handle.runtime().upsert_capture_exchange(input).await?;
    assert_eq!(session_activity(f).await?, inserted);
    let mut updated = input.clone();
    updated.metadata["capture_changed"] = json!(true);
    f.handle.runtime().upsert_capture_exchange(&updated).await?;
    assert!(session_activity(f).await? > inserted);
    let observations = sqlx::query_scalar!(
        "SELECT count(*) AS \"count!\" FROM runtime.session_facts WHERE session_id = $1",
        f.session_db
    )
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(observations, 0);
    Ok(())
}

async fn session_activity(f: &Fixture) -> anyhow::Result<chrono::DateTime<Utc>> {
    Ok(sqlx::query_scalar!(
        "SELECT last_seen FROM sessions.sessions WHERE id = $1",
        f.session_db
    )
    .fetch_one(f.handle.pool())
    .await?)
}

#[tokio::test]
async fn capture_invalid_references_and_conflicting_replays_are_rejected() -> anyhow::Result<()> {
    let f = setup().await?;
    let first = f.handle.runtime().create_capture_document(None).await?;
    let chunks = [CaptureChunkInput::New(b"complete source".to_vec())];
    let ids = f
        .handle
        .runtime()
        .append_capture_chunks(first, 0, &chunks)
        .await?;
    assert!(
        f.handle
            .runtime()
            .append_capture_chunks(first, 0, &[CaptureChunkInput::New(b"changed".to_vec())])
            .await
            .is_err()
    );
    assert!(
        f.handle
            .runtime()
            .append_capture_chunks(first, 2, &chunks)
            .await
            .is_err()
    );
    f.handle
        .runtime()
        .finish_capture_document(first, false)
        .await?;
    f.handle
        .runtime()
        .finish_capture_document(first, false)
        .await?;
    assert!(
        f.handle
            .runtime()
            .finish_capture_document(first, true)
            .await
            .is_err()
    );
    assert!(
        f.handle
            .runtime()
            .append_capture_chunks(first, 1, &chunks)
            .await
            .is_err()
    );
    let no_predecessor = f.handle.runtime().create_capture_document(None).await?;
    assert!(
        f.handle
            .runtime()
            .append_capture_chunks(no_predecessor, 0, &[CaptureChunkInput::Existing(ids[0])])
            .await
            .is_err()
    );
    let mut owner = exchange(&f, first);
    f.handle.runtime().upsert_capture_exchange(&owner).await?;
    owner.id = Uuid::new_v4();
    assert!(
        f.handle
            .runtime()
            .upsert_capture_exchange(&owner)
            .await
            .is_err()
    );
    let derived = f
        .handle
        .runtime()
        .create_capture_document(Some(first))
        .await?;
    f.handle
        .runtime()
        .append_capture_chunks(derived, 0, &[CaptureChunkInput::Existing(ids[0])])
        .await?;
    let mut wrong_scope = exchange(&f, derived);
    wrong_scope.session_uuid = None;
    wrong_scope.provider_thread_id = Some("different-thread".into());
    assert!(
        f.handle
            .runtime()
            .upsert_capture_exchange(&wrong_scope)
            .await
            .is_err()
    );
    assert!(
        f.handle
            .runtime()
            .get_capture_exchange(wrong_scope.id)
            .await?
            .is_none()
    );
    assert_eq!(
        f.handle
            .runtime()
            .previous_capture_request(Some(f.session), Some("test-thread"))
            .await?,
        None
    );
    f.cleanup().await
}

#[tokio::test]
async fn capture_session_survives_existing_automatic_retention() -> anyhow::Result<()> {
    let f = setup().await?;
    let document = f.handle.runtime().create_capture_document(None).await?;
    f.handle
        .runtime()
        .append_capture_chunks(
            document,
            0,
            &[CaptureChunkInput::New(b"permanent capture".to_vec())],
        )
        .await?;
    f.handle
        .runtime()
        .finish_capture_document(document, true)
        .await?;
    let input = exchange(&f, document);
    f.handle.runtime().upsert_capture_exchange(&input).await?;
    sqlx::query!(
        "INSERT INTO runtime.session_identities (session_id) VALUES ($1)",
        f.session_db
    )
    .execute(f.handle.pool())
    .await?;
    let future = Utc::now() + Duration::days(400);
    f.handle
        .runtime()
        .sweep_runtime_observations(future, 100)
        .await?;
    assert_eq!(
        f.handle
            .sweep_expired_sessions(std::time::Duration::from_secs(1), future)
            .await?,
        0
    );
    sqlx::query!(
        "UPDATE sessions.sessions SET ttl_override = interval '1 second' WHERE id = $1",
        f.session_db
    )
    .execute(f.handle.pool())
    .await?;
    assert_eq!(
        f.handle
            .sweep_expired_sessions(std::time::Duration::from_secs(1), future)
            .await?,
        0
    );
    assert!(
        f.handle
            .runtime()
            .get_capture_exchange(input.id)
            .await?
            .is_some()
    );
    assert_eq!(
        f.handle
            .runtime()
            .capture_document_chunks(document, 0, 1)
            .await?[0]
            .body,
        b"permanent capture"
    );
    f.cleanup().await
}
