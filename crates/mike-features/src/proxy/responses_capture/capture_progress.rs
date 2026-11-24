use anyhow::Context as _;

use super::{ExchangeRecord, ResponsesState, manifest};
use crate::proxy::responses_capture::spool::Manifest;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::runtime_repo::RuntimePersist as _;

pub(super) fn restore(current: &mut Manifest, previous: Option<&Manifest>) {
    let Some(previous) = previous else { return };
    current.exchange.request_document_id = previous.exchange.request_document_id;
    current.exchange.sent_document_id = previous.exchange.sent_document_id;
    current.exchange.response_document_id = previous.exchange.response_document_id;
    current.request_predecessor = previous.request_predecessor;
    current.response_predecessor = previous.response_predecessor;
    current.sent_changed = previous.sent_changed;
    current.sent_complete = previous.sent_complete;
    current.request_persisted = previous.request_persisted;
    current.sent_persisted = previous.sent_persisted;
    current
        .injected_messages
        .clone_from(&previous.injected_messages);
    current.upstream_accepted = previous.upstream_accepted;
    for key in [
        "request_stored_encoding",
        "sent_stored_encoding",
        "request_decoding_error",
    ] {
        if let Some(value) = previous.exchange.metadata.get(key) {
            current.exchange.metadata[key] = value.clone();
        }
    }
    current.exchange.metadata["sent_changed"] = serde_json::json!(current.sent_changed);
    current.exchange.metadata["upstream_accepted"] = serde_json::json!(current.upstream_accepted);
    current.exchange.metadata["injected_messages"] = serde_json::json!(current.injected_messages);
}

pub(crate) async fn pre_forward(
    state: &ResponsesState,
    record: &mut ExchangeRecord,
) -> anyhow::Result<()> {
    let Some(injection) = &record.injection else {
        return Ok(());
    };
    let mut progress = manifest(record, None);
    progress.sent_changed = true;
    progress.sent_complete = true;
    progress.injected_messages = injection.message_ids();
    progress.exchange.metadata["sent_stored_encoding"] = serde_json::json!("identity");
    progress.exchange.metadata["sent_changed"] = serde_json::json!(true);
    progress.exchange.metadata["injected_messages"] = serde_json::json!(progress.injected_messages);
    record.progress = Some(progress);
    record
        .request_body
        .wait()
        .await
        .map_err(anyhow::Error::msg)
        .context("finish original request staging")?;
    let persist = state
        .persist
        .as_ref()
        .context("injection requires capture persistence")?;
    let _slot = state
        .slots
        .clone()
        .acquire_owned()
        .await
        .context("acquire capture persistence slot")?;
    let progress = record
        .progress
        .as_mut()
        .context("injection capture progress missing")?;
    record
        .spool
        .write_manifest(progress)
        .await
        .map_err(anyhow::Error::msg)
        .context("save injected request staging")?;
    crate::proxy::responses_capture::capture_write::persist(
        persist,
        &record.spool.directory,
        progress,
    )
    .await
    .map_err(anyhow::Error::msg)
    .context("persist original and injected request")
}

pub(crate) async fn accepted(
    state: &ResponsesState,
    record: &mut ExchangeRecord,
) -> anyhow::Result<()> {
    if !record
        .progress
        .as_ref()
        .is_some_and(|progress| progress.sent_changed)
    {
        return Ok(());
    }
    let mut progress = manifest(record, None);
    progress.upstream_accepted = true;
    progress.exchange.metadata["upstream_accepted"] = serde_json::json!(true);
    record.progress = Some(progress);
    let progress = record
        .progress
        .as_ref()
        .context("accepted injection capture missing")?;
    record
        .spool
        .write_manifest(progress)
        .await
        .map_err(anyhow::Error::msg)
        .context("save upstream acceptance evidence")?;
    let persist = state
        .persist
        .as_ref()
        .context("accepted injection requires capture persistence")?;
    persist
        .runtime()
        .upsert_capture_exchange(&progress.exchange)
        .await
        .context("save accepted exchange metadata")?;
    finish_delivery(persist, progress).await
}

pub(crate) async fn finish_delivery(
    persist: &PersistHandle,
    progress: &Manifest,
) -> anyhow::Result<()> {
    if progress.injected_messages.is_empty() {
        return Ok(());
    }
    if !progress.upstream_accepted {
        for message in &progress.injected_messages {
            persist
                .launch()
                .unclaim_message(*message)
                .await
                .context("release unaccepted Codex mail claim")?;
        }
        return Ok(());
    }
    let session = progress
        .exchange
        .session_uuid
        .context("accepted mail has no verified session")?;
    let delivered = persist
        .launch()
        .record_capture_delivery(session, progress.exchange.id, &progress.injected_messages)
        .await
        .context("record accepted Codex mail delivery")?;
    anyhow::ensure!(
        progress
            .injected_messages
            .iter()
            .all(|id| delivered.contains(id)),
        "accepted Codex mail delivery evidence is incomplete"
    );
    Ok(())
}

#[cfg(test)]
#[path = "capture_progress_tests.rs"]
mod tests;
