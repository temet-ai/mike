use serde_json::json;
use uuid::Uuid;

use super::*;
use crate::proxy::responses_capture::relay_test_support::{body, harness};
use crate::proxy::responses_capture::spool::Spool;

fn staged(id: Uuid, session: Uuid) -> anyhow::Result<Manifest> {
    Ok(serde_json::from_value(json!({
        "exchange": {
            "id":id,"session_uuid":session,"provider_thread_id":null,
            "created_at":chrono::Utc::now(),"metadata":{},
            "request_document_id":0,"sent_document_id":0,"response_document_id":null,
            "status":"pending"
        },
        "request_predecessor":null,"request_complete":true,
        "response_complete":false,"response_received":false
    }))?)
}

#[test]
fn progress_restore_after_injection_taken_keeps_document_identity_and_encoding()
-> anyhow::Result<()> {
    let mut previous = staged(Uuid::new_v4(), Uuid::new_v4())?;
    assert!(!previous.sent_changed);
    assert!(!previous.upstream_accepted);
    previous.exchange.request_document_id = 11;
    previous.exchange.sent_document_id = 12;
    previous.exchange.response_document_id = Some(13);
    previous.request_predecessor = Some(9);
    previous.response_predecessor = Some(10);
    previous.request_persisted = true;
    previous.sent_persisted = true;
    previous.sent_complete = true;
    previous.sent_changed = true;
    previous.upstream_accepted = true;
    previous.injected_messages.push(Uuid::new_v4());
    previous.exchange.metadata =
        json!({"request_stored_encoding":"identity", "sent_stored_encoding":"identity"});
    let mut current = staged(previous.exchange.id, Uuid::new_v4())?;
    current.exchange.metadata = json!({"request_stored_encoding":"zstd", "sent_stored_encoding":"zstd", "response_status":200});
    restore(&mut current, Some(&previous));
    assert_eq!(current.exchange.request_document_id, 11);
    assert_eq!(current.exchange.sent_document_id, 12);
    assert_eq!(current.exchange.response_document_id, Some(13));
    assert_eq!(current.request_predecessor, Some(9));
    assert_eq!(current.response_predecessor, Some(10));
    assert!(current.request_persisted && current.sent_persisted && current.sent_complete);
    assert!(current.sent_changed && current.upstream_accepted);
    assert_eq!(current.injected_messages, previous.injected_messages);
    assert_eq!(
        current.exchange.metadata["request_stored_encoding"],
        "identity"
    );
    assert_eq!(
        current.exchange.metadata["sent_stored_encoding"],
        "identity"
    );
    assert_eq!(current.exchange.metadata["response_status"], 200);
    Ok(())
}

#[tokio::test]
async fn db_changed_request_persisted_before_forward_reuses_original_chunks_and_survives_replay()
-> anyhow::Result<()> {
    let h = harness("http://127.0.0.1:1", "http://127.0.0.1:1")
        .await?
        .ok_or_else(|| anyhow::anyhow!("capture test requires the test database"))?;
    let spool = Spool::create(h.home.path()).await?;
    let session = Uuid::new_v4();
    h.persist.ensure_runtime_session(session).await?;
    let mut progress = staged(spool.id, session)?;
    progress.sent_changed = true;
    progress.sent_complete = true;
    let original = vec![b'a'; 512 * 1024];
    let mut sent = original.clone();
    sent.extend_from_slice(b"external context");
    tokio::fs::write(spool.directory.join("request"), &original).await?;
    tokio::fs::write(spool.directory.join("sent"), &sent).await?;
    crate::proxy::responses_capture::capture_write::persist(
        &h.persist,
        &spool.directory,
        &mut progress,
    )
    .await
    .map_err(anyhow::Error::msg)?;
    let request_id = progress.exchange.request_document_id;
    let sent_id = progress.exchange.sent_document_id;
    assert_ne!(request_id, sent_id);
    assert_eq!(body(&h, request_id).await?, original);
    assert_eq!(body(&h, sent_id).await?, sent);
    let originals = h
        .persist
        .runtime()
        .capture_document_chunks(request_id, 0, 100)
        .await?;
    let changed = h
        .persist
        .runtime()
        .capture_document_chunks(sent_id, 0, 100)
        .await?;
    assert!(
        changed
            .iter()
            .any(|chunk| originals.iter().any(|original| chunk.id == original.id))
    );
    for name in ["request", "sent"] {
        tokio::fs::remove_file(spool.directory.join(name)).await?;
    }
    crate::proxy::responses_capture::capture_write::persist(
        &h.persist,
        &spool.directory,
        &mut progress,
    )
    .await
    .map_err(anyhow::Error::msg)?;
    assert_eq!(progress.exchange.request_document_id, request_id);
    assert_eq!(progress.exchange.sent_document_id, sent_id);
    assert!(progress.request_persisted && progress.sent_persisted);
    Ok(())
}

#[tokio::test]
async fn db_recovery_acceptance_marker_records_receipt_while_rejected_mail_is_released()
-> anyhow::Result<()> {
    let h = harness("http://127.0.0.1:1", "http://127.0.0.1:1")
        .await?
        .ok_or_else(|| anyhow::anyhow!("capture test requires the test database"))?;
    let root = h
        .state
        .capture_root
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("capture root missing"))?;
    for accepted in [false, true] {
        let spool = Spool::create(root).await?;
        let session = Uuid::new_v4();
        let message = Uuid::new_v4();
        h.persist.ensure_runtime_session(session).await?;
        h.persist
            .launch()
            .insert_message(message, session, None, "external context")
            .await?
            .ok_or_else(|| anyhow::anyhow!("mail not inserted"))?;
        assert!(h.persist.launch().claim_message(message).await?);
        let mut progress = staged(spool.id, session)?;
        progress.sent_changed = true;
        progress.sent_complete = true;
        progress.injected_messages.push(message);
        tokio::fs::write(spool.directory.join("request"), b"original").await?;
        tokio::fs::write(spool.directory.join("sent"), b"original external context").await?;
        crate::proxy::responses_capture::capture_write::persist(
            &h.persist,
            &spool.directory,
            &mut progress,
        )
        .await
        .map_err(anyhow::Error::msg)?;
        progress.upstream_accepted = accepted;
        spool
            .write_manifest(&progress)
            .await
            .map_err(anyhow::Error::msg)?;
        let directory = spool.directory.clone();
        let id = spool.id;
        drop(spool);
        crate::proxy::responses_capture::recovery::recover(h.state.clone()).await;
        assert!(!directory.exists());
        let capture = h
            .persist
            .runtime()
            .get_capture_exchange(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("capture missing"))?;
        assert_eq!(
            capture.request_document_id,
            progress.exchange.request_document_id
        );
        assert_eq!(capture.sent_document_id, progress.exchange.sent_document_id);
        assert!(
            h.persist
                .runtime()
                .capture_document(capture.request_document_id)
                .await?
                .is_some_and(|document| document.complete)
        );
        let receipts = h
            .persist
            .launch()
            .fetch_capture_delivery_evidence(session, message)
            .await?;
        assert_eq!(receipts.len(), usize::from(accepted));
        if accepted {
            assert_eq!(receipts[0].exchange_uuid, id);
            finish_delivery(&h.persist, &progress).await?;
            assert_eq!(
                h.persist
                    .launch()
                    .fetch_capture_delivery_evidence(session, message)
                    .await?
                    .len(),
                1
            );
        } else {
            assert!(h.persist.launch().claim_message(message).await?);
        }
    }
    Ok(())
}

#[tokio::test]
async fn db_recovery_missing_delivery_evidence_retains_the_accepted_spool() -> anyhow::Result<()> {
    let h = harness("http://127.0.0.1:1", "http://127.0.0.1:1")
        .await?
        .ok_or_else(|| anyhow::anyhow!("capture test requires the test database"))?;
    let root = h
        .state
        .capture_root
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("capture root missing"))?;
    let spool = Spool::create(root).await?;
    let session = Uuid::new_v4();
    h.persist.ensure_runtime_session(session).await?;
    let mut progress = staged(spool.id, session)?;
    progress.sent_changed = true;
    progress.sent_complete = true;
    progress.upstream_accepted = true;
    progress.injected_messages.push(Uuid::new_v4());
    tokio::fs::write(spool.directory.join("request"), b"original").await?;
    tokio::fs::write(spool.directory.join("sent"), b"original external context").await?;
    spool
        .write_manifest(&progress)
        .await
        .map_err(anyhow::Error::msg)?;
    let directory = spool.directory.clone();
    drop(spool);
    crate::proxy::responses_capture::recovery::recover(h.state.clone()).await;
    assert!(directory.join("manifest.json").exists());
    assert!(finish_delivery(&h.persist, &progress).await.is_err());
    Ok(())
}
