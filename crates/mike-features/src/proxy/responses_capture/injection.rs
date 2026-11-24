mod selection;
#[cfg(test)]
mod tests;

use std::path::PathBuf;

use mike_codex::injection::{InjectionOutcome, RequestInfo, RequestInspection};

use super::ResponsesState;
use super::record::ExchangeRecord;

#[derive(Debug)]
pub(super) struct PendingInjection {
    session: String,
    selected: Vec<selection::Selected>,
}

impl PendingInjection {
    pub(super) fn message_ids(&self) -> Vec<uuid::Uuid> {
        self.selected
            .iter()
            .filter_map(|item| selection::message_id(&item.reminder))
            .collect()
    }
}

pub(super) async fn prepare(
    state: &ResponsesState,
    record: &mut ExchangeRecord,
) -> Result<(), String> {
    let Some(host) = &state.host else {
        return Ok(());
    };
    let Some(session) = eligible_session(record) else {
        return Ok(());
    };
    let (path, info) = match inspect(record).await {
        Ok(Some(inspected)) => inspected,
        Ok(None) => return Ok(()),
        Err(error) => {
            tracing::warn!(capture = %record.spool.id, error, "Codex request stays unchanged; context remains pending");
            return Ok(());
        }
    };
    let selected = selection::select(host, &session, info.last_input_is_tool_result).await;
    if selected.is_empty() {
        return Ok(());
    }
    let context = selection::context(&session, &selected);
    record.injection = Some(PendingInjection { session, selected });
    write_context(state, record, path, context).await
}

async fn write_context(
    state: &ResponsesState,
    record: &mut ExchangeRecord,
    path: PathBuf,
    context: String,
) -> Result<(), String> {
    let target = record.spool.directory.join("sent");
    match splice(path, target, context).await {
        Ok(InjectionOutcome::Injected { .. }) => Ok(()),
        Ok(InjectionOutcome::Unchanged(reason)) => {
            finish(state, record, false).await;
            tracing::warn!(capture = %record.spool.id, reason = reason.as_str(), "Codex context remains pending");
            Ok(())
        }
        Err(error) => {
            finish(state, record, false).await;
            Err(error)
        }
    }
}

fn eligible_session(record: &ExchangeRecord) -> Option<String> {
    if record.method != "POST"
        || !record
            .path
            .split('?')
            .next()
            .is_some_and(|path| path.ends_with("/responses"))
        || record
            .correlation
            .request_kind
            .as_deref()
            .is_some_and(|kind| kind != "turn")
    {
        return None;
    }
    record.correlation.session_uuid.map(|id| id.to_string())
}

async fn inspect(record: &ExchangeRecord) -> Result<Option<(PathBuf, RequestInfo)>, String> {
    let path = decoded_request(record).await?;
    let input_path = path.clone();
    let inspection = tokio::task::spawn_blocking(move || {
        let mut input = std::fs::File::open(input_path)?;
        mike_codex::injection::inspect_request(&mut input)
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())?;
    Ok(match inspection {
        RequestInspection::Supported(info) => Some((path, info)),
        RequestInspection::Unsupported(_) => None,
    })
}

async fn decoded_request(record: &ExchangeRecord) -> Result<PathBuf, String> {
    let encoding = record
        .request_body_encoding
        .as_deref()
        .unwrap_or("identity");
    if encoding.is_empty() || encoding.eq_ignore_ascii_case("identity") {
        return Ok(record.request_body.path.clone());
    }
    let path = record.spool.directory.join("injection.decoded");
    let input = tokio::fs::File::open(&record.request_body.path)
        .await
        .map_err(|e| e.to_string())?;
    let mut output = tokio::fs::File::create(&path)
        .await
        .map_err(|e| e.to_string())?;
    mike_codex::capture::decode_body(encoding, input, &mut output)
        .await
        .map_err(|e| e.to_string())?;
    output.sync_all().await.map_err(|e| e.to_string())?;
    Ok(path)
}

async fn splice(
    input: PathBuf,
    target: PathBuf,
    context: String,
) -> Result<InjectionOutcome, String> {
    tokio::task::spawn_blocking(move || {
        let mut input = std::fs::File::open(input)?;
        let mut output = std::io::BufWriter::new(std::fs::File::create(&target)?);
        let outcome = mike_codex::injection::inject_context(&mut input, &mut output, &context)?;
        output.get_ref().sync_all()?;
        Ok::<_, mike_codex::injection::InjectionError>(outcome)
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())
}

pub(super) async fn finish(state: &ResponsesState, record: &mut ExchangeRecord, accepted: bool) {
    if let Some(pending) = record.injection.take()
        && let Some(host) = &state.host
    {
        selection::finish(host, &pending.session, pending.selected, accepted).await;
    }
}
