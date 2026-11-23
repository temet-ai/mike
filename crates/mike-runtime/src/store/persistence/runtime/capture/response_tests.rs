use super::{CaptureChunkInput, Fixture, exchange, setup};
use crate::store::persistence::runtime_repo::RuntimePersist as _;

#[tokio::test]
async fn captured_response_predecessor_reuses_bytes_and_keeps_incomplete_evidence()
-> anyhow::Result<()> {
    let f = setup().await?;
    let first = f.handle.runtime().create_capture_document(None).await?;
    let prefix = b"event: response.created\ndata: {\"tools\":[]}\n\n".repeat(2048);
    let ids = f
        .handle
        .runtime()
        .append_capture_chunks(
            first,
            0,
            &[
                CaptureChunkInput::New(prefix.clone()),
                CaptureChunkInput::New(b"data: first answer\n\n".to_vec()),
            ],
        )
        .await?;
    f.handle
        .runtime()
        .finish_capture_document(first, true)
        .await?;
    attach_response(&f, first, "complete").await?;
    assert_predecessor(&f, first).await?;
    let second = f
        .handle
        .runtime()
        .create_capture_document(Some(first))
        .await?;
    let suffix = b"data: partial second answer";
    let reused = f
        .handle
        .runtime()
        .append_capture_chunks(
            second,
            0,
            &[
                CaptureChunkInput::Existing(ids[0]),
                CaptureChunkInput::New(suffix.to_vec()),
            ],
        )
        .await?;
    assert_eq!(reused[0], ids[0]);
    f.handle
        .runtime()
        .finish_capture_document(second, false)
        .await?;
    attach_response(&f, second, "aborted").await?;
    assert_predecessor(&f, second).await?;
    let body: Vec<u8> = f
        .handle
        .runtime()
        .capture_document_chunks(second, 0, 10)
        .await?
        .into_iter()
        .flat_map(|chunk| chunk.body)
        .collect();
    assert_eq!(body, [prefix.as_slice(), suffix].concat());
    assert_newer_unusable_responses_are_skipped(&f, second, ids[0]).await?;
    assert_foreign_response_predecessor_is_rejected(&f, second, ids[0]).await?;
    f.cleanup().await
}

async fn attach_response(f: &Fixture, response: i64, status: &str) -> anyhow::Result<()> {
    let request = f.handle.runtime().create_capture_document(None).await?;
    f.handle
        .runtime()
        .finish_capture_document(request, true)
        .await?;
    let mut input = exchange(f, request);
    input.response_document_id = Some(response);
    input.status = status.into();
    f.handle.runtime().upsert_capture_exchange(&input).await?;
    Ok(())
}

async fn assert_predecessor(f: &Fixture, expected: i64) -> anyhow::Result<()> {
    assert_eq!(
        f.handle
            .runtime()
            .previous_capture_response(Some(f.session), Some("test-thread"))
            .await?,
        Some(expected)
    );
    assert_eq!(
        f.handle
            .runtime()
            .previous_capture_response(None, Some("test-thread"))
            .await?,
        Some(expected)
    );
    assert_eq!(
        f.handle
            .runtime()
            .previous_capture_response(Some(uuid::Uuid::new_v4()), Some("test-thread"))
            .await?,
        None
    );
    assert_eq!(
        f.handle
            .runtime()
            .previous_capture_response(None, Some("different-thread"))
            .await?,
        None
    );
    assert_eq!(
        f.handle
            .runtime()
            .previous_capture_response(None, None)
            .await?,
        None
    );
    Ok(())
}

async fn assert_newer_unusable_responses_are_skipped(
    f: &Fixture,
    useful: i64,
    chunk: i64,
) -> anyhow::Result<()> {
    let empty = f.handle.runtime().create_capture_document(None).await?;
    f.handle
        .runtime()
        .finish_capture_document(empty, true)
        .await?;
    attach_response(f, empty, "complete").await?;
    assert_predecessor(f, useful).await?;
    let unfinished = f
        .handle
        .runtime()
        .create_capture_document(Some(useful))
        .await?;
    f.handle
        .runtime()
        .append_capture_chunks(unfinished, 0, &[CaptureChunkInput::Existing(chunk)])
        .await?;
    attach_response(f, unfinished, "pending").await?;
    assert_predecessor(f, useful).await
}

async fn assert_foreign_response_predecessor_is_rejected(
    f: &Fixture,
    previous: i64,
    chunk: i64,
) -> anyhow::Result<()> {
    let response = f
        .handle
        .runtime()
        .create_capture_document(Some(previous))
        .await?;
    f.handle
        .runtime()
        .append_capture_chunks(response, 0, &[CaptureChunkInput::Existing(chunk)])
        .await?;
    let request = f.handle.runtime().create_capture_document(None).await?;
    let mut input = exchange(f, request);
    input.response_document_id = Some(response);
    input.session_uuid = None;
    input.provider_thread_id = Some("different-thread".into());
    assert!(matches!(
        f.handle.runtime().upsert_capture_exchange(&input).await,
        Err(crate::store::persistence::PersistError::DataInvariant(_))
    ));
    Ok(())
}
