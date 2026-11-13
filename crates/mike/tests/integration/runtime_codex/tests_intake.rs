use mike::store::persistence::runtime_repo::RuntimePersist as _;
use serde_json::json;
use uuid::Uuid;

use mike::runtime::RuntimeObservation;
use mike::runtime::codex::{ExclusionReason, IntakeContext, RequestId, RpcScope};
use mike::store::persistence::runtime::RuntimeWriteOutcome;

use super::test_support::{captured, count, database, delta, input, input_at, receipt_id};
use super::{ClassificationProvenance, CodexCaptureOutcome, CodexIntakeContext, RawEventPage};

#[tokio::test]
async fn trusted_capture_exact_retry_preserves_bytes_context_id_and_sequence() -> anyhow::Result<()>
{
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let mut request = input(namespace.id, delta("a\0b"))?;
    request
        .envelope
        .extra
        .insert("future".into(), json!({"value":"\0"}));
    let first = captured(
        handle
            .runtime()
            .capture_codex_receipt(request.clone())
            .await?,
    )?;
    assert_eq!(first.capture_sequence, Some(1));
    let retry = handle
        .runtime()
        .capture_codex_receipt(request.clone())
        .await?;
    assert!(matches!(
        &retry,
        CodexCaptureOutcome::Captured {
            outcome: RuntimeWriteOutcome::Unchanged,
            ..
        }
    ));
    assert_eq!(captured(retry)?, first);
    assert_eq!(
        handle
            .runtime()
            .runtime_receipt(receipt_id(&request))
            .await?,
        Some(first)
    );
    assert_eq!(count(&handle, namespace.id, "events").await?, 1);
    assert_eq!(count(&handle, namespace.id, "context").await?, 1);
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn trusted_receipt_changes_to_bytes_envelope_or_context_conflict() -> anyhow::Result<()> {
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let request = input(namespace.id, delta("A"))?;
    let original = captured(
        handle
            .runtime()
            .capture_codex_receipt(request.clone())
            .await?,
    )?;
    let mut changed = request.clone();
    changed.raw_payload.push(b' ');
    assert!(
        handle
            .runtime()
            .capture_codex_receipt(changed)
            .await
            .is_err()
    );
    changed = request.clone();
    changed.envelope.source_cursor = Some("another".into());
    assert!(
        handle
            .runtime()
            .capture_codex_receipt(changed)
            .await
            .is_err()
    );
    changed = request.clone();
    changed.context = CodexIntakeContext::new(
        IntakeContext::default(),
        Uuid::new_v4(),
        1,
        ClassificationProvenance::ReviewedNotification,
    )?;
    assert!(
        handle
            .runtime()
            .capture_codex_receipt(changed)
            .await
            .is_err()
    );
    assert_eq!(
        handle
            .runtime()
            .runtime_receipt(receipt_id(&request))
            .await?,
        Some(original)
    );
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn rpc_context_changes_preserve_typed_ids_and_never_reclassify_retries() -> anyhow::Result<()>
{
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let scope = RpcScope {
        request_id: RequestId::String("7\0x".into()),
        method: "thread/read".into(),
        thread_id: Some("thread".into()),
        turn_id: None,
    };
    let epoch = Uuid::new_v4();
    let mut request = input_at(
        namespace.id,
        serde_json::to_vec(&json!({"id":"7\0x", "result":{"thread":{"id":"thread"}}}))?,
        epoch,
        0,
        IntakeContext::new(Some(&scope)),
    )?;
    let original = captured(
        handle
            .runtime()
            .capture_codex_receipt(request.clone())
            .await?,
    )?;
    for changed_scope in [
        RpcScope {
            method: "thread/items/list".into(),
            ..scope.clone()
        },
        RpcScope {
            thread_id: Some("different".into()),
            ..scope.clone()
        },
        RpcScope {
            turn_id: Some("different".into()),
            ..scope.clone()
        },
    ] {
        request.context = CodexIntakeContext::new(
            IntakeContext::new(Some(&changed_scope)),
            epoch,
            0,
            ClassificationProvenance::ReviewedOrdinaryRoute,
        )?;
        assert!(
            handle
                .runtime()
                .capture_codex_receipt(request.clone())
                .await
                .is_err()
        );
    }
    let wrong_id = RpcScope {
        request_id: RequestId::Integer(7),
        ..scope
    };
    request.context = CodexIntakeContext::new(
        IntakeContext::new(Some(&wrong_id)),
        Uuid::new_v4(),
        0,
        ClassificationProvenance::ReviewedRpcResponse,
    )?;
    assert!(matches!(
        handle
            .runtime()
            .capture_codex_receipt(request.clone())
            .await?,
        CodexCaptureOutcome::Excluded(ExclusionReason::UncorrelatedResponse)
    ));
    assert_eq!(
        handle
            .runtime()
            .runtime_receipt(receipt_id(&request))
            .await?,
        Some(original)
    );
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn auth_ambiguous_and_unknown_unclassified_frames_never_enter_capture() -> anyhow::Result<()>
{
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    for bytes in [
        br#"{"method":"account/login/start","params":{"token":"secret-fixture"}}"#.to_vec(),
        br#"{"id":7,"result":{"token":"secret-fixture"}}"#.to_vec(),
        br#"{"method":"account/login/start","method":"item/started"}"#.to_vec(),
    ] {
        let request = input_at(
            namespace.id,
            bytes,
            Uuid::new_v4(),
            0,
            IntakeContext::ordinary(),
        )?;
        assert!(matches!(
            handle.runtime().capture_codex_receipt(request).await?,
            CodexCaptureOutcome::Excluded(_)
        ));
    }
    for bytes in [
        vec![0xff, 0, 0xfe],
        br#"{"method":"future/unknown"}"#.to_vec(),
    ] {
        let request = input_at(
            namespace.id,
            bytes,
            Uuid::new_v4(),
            0,
            IntakeContext::default(),
        )?;
        assert!(matches!(
            handle.runtime().capture_codex_receipt(request).await?,
            CodexCaptureOutcome::Excluded(ExclusionReason::UnclassifiedTraffic)
        ));
    }
    for table in ["events", "context", "effects", "applications"] {
        assert_eq!(count(&handle, namespace.id, table).await?, 0);
    }
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn legacy_raw_receipts_remain_unclassified_and_cannot_gain_context() -> anyhow::Result<()> {
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let request = input(namespace.id, delta("legacy"))?;
    let raw = RuntimeObservation::new(request.envelope.clone(), request.raw_payload.clone())?;
    handle.runtime().append_runtime_observation(&raw).await?;
    assert!(
        handle
            .runtime()
            .capture_codex_receipt(request.clone())
            .await
            .is_err()
    );
    let rows = handle
        .runtime()
        .list_codex_unclassified_receipts(RawEventPage {
            namespace_id: namespace.id,
            after_event_id: 0,
            limit: 10,
        })
        .await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].observation, raw);
    assert!(rows[0].context.is_none());
    let generation = handle
        .runtime()
        .begin_codex_projection_generation(namespace.id, Uuid::new_v4())
        .await?;
    assert!(matches!(
        handle
            .runtime()
            .apply_next_codex_projection(generation.generation_id)
            .await?,
        super::CodexProjectionApplyOutcome::CaughtUp(_)
    ));
    assert_eq!(count(&handle, namespace.id, "applications").await?, 0);
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn concurrent_identical_capture_writes_one_raw_receipt_and_context() -> anyhow::Result<()> {
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let request = input(namespace.id, delta("A"))?;
    let (first, second) = tokio::join!(
        handle.runtime().capture_codex_receipt(request.clone()),
        handle.runtime().capture_codex_receipt(request)
    );
    assert_eq!(captured(first?)?, captured(second?)?);
    assert_eq!(count(&handle, namespace.id, "events").await?, 1);
    assert_eq!(count(&handle, namespace.id, "context").await?, 1);
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn transport_duplicates_regressions_and_gaps_are_detected() -> anyhow::Result<()> {
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let epoch = Uuid::new_v4();
    let bytes = serde_json::to_vec(&delta("A"))?;
    let first = input_at(
        namespace.id,
        bytes.clone(),
        epoch,
        5,
        IntakeContext::ordinary(),
    )?;
    handle.runtime().capture_codex_receipt(first).await?;
    for ordinal in [5, 4] {
        let request = input_at(
            namespace.id,
            bytes.clone(),
            epoch,
            ordinal,
            IntakeContext::ordinary(),
        )?;
        assert!(
            handle
                .runtime()
                .capture_codex_receipt(request)
                .await
                .is_err()
        );
    }
    let gap = input_at(
        namespace.id,
        bytes.clone(),
        epoch,
        7,
        IntakeContext::ordinary(),
    )?;
    assert_eq!(
        captured(handle.runtime().capture_codex_receipt(gap).await?)?.transport_gap,
        Some(true)
    );
    let next = input_at(namespace.id, bytes, epoch, 8, IntakeContext::ordinary())?;
    let next = captured(handle.runtime().capture_codex_receipt(next).await?)?;
    assert_eq!(next.transport_gap, Some(false));
    assert_eq!(next.capture_sequence, Some(3));
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}
