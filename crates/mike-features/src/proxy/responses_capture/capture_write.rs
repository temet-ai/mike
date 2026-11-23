use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use mike_codex::capture::{Chunker, KnownChunk, matching_chunk_id};
use tokio::io::AsyncReadExt;

use super::spool::{Manifest, write_manifest};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::runtime::capture::{CaptureChunkInput, KnownCaptureChunk};
use crate::store::persistence::runtime_repo::RuntimePersist as _;

const CANDIDATES: i64 = 8;
const WRITE_BATCH: usize = 32;

pub(super) async fn persist(
    persist: &PersistHandle,
    directory: &Path,
    manifest: &mut Manifest,
) -> Result<(), String> {
    if manifest.exchange.session_uuid.is_none()
        && (manifest.identity.launch.is_some() || manifest.identity.thread.is_some())
    {
        return Err("capture awaits resolution of its proxy session identity".into());
    }
    let request_path = decode(directory, "request", manifest).await?;
    let response_path = if manifest.response_received {
        Some(decode(directory, "response", manifest).await?)
    } else {
        None
    };
    write_manifest(directory, manifest).await?;
    let predecessor = prepare_request(persist, directory, manifest).await?;
    document(
        persist,
        manifest.exchange.request_document_id,
        predecessor,
        &request_path,
        manifest.request_complete,
    )
    .await?;
    if let Some(response_path) = response_path {
        persist_response(persist, directory, manifest, &response_path).await?;
    }
    persist
        .runtime()
        .upsert_capture_exchange(&manifest.exchange)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn prepare_request(
    persist: &PersistHandle,
    directory: &Path,
    manifest: &mut Manifest,
) -> Result<Option<i64>, String> {
    if manifest.exchange.request_document_id == 0 {
        let previous = persist
            .runtime()
            .previous_capture_request(
                manifest.exchange.session_uuid,
                manifest.exchange.provider_thread_id.as_deref(),
            )
            .await
            .map_err(|error| error.to_string())?;
        let id = persist
            .runtime()
            .create_capture_document(previous)
            .await
            .map_err(|error| error.to_string())?;
        manifest.exchange.request_document_id = id;
        manifest.exchange.sent_document_id = id;
        manifest.request_predecessor = previous;
        write_manifest(directory, manifest).await?;
    }
    Ok(manifest.request_predecessor)
}

async fn persist_response(
    persist: &PersistHandle,
    directory: &Path,
    manifest: &mut Manifest,
    response_path: &Path,
) -> Result<(), String> {
    let id = match manifest.exchange.response_document_id {
        Some(id) => id,
        None => {
            let id = persist
                .runtime()
                .create_capture_document(None)
                .await
                .map_err(|error| error.to_string())?;
            manifest.exchange.response_document_id = Some(id);
            write_manifest(directory, manifest).await?;
            id
        }
    };
    document(persist, id, None, response_path, manifest.response_complete).await
}

struct Writer<'a> {
    persist: &'a PersistHandle,
    document: i64,
    predecessor: Option<i64>,
    candidates: VecDeque<KnownCaptureChunk>,
    anchor: Vec<KnownCaptureChunk>,
    next_candidate: i64,
    ordinal: i64,
    pending: Vec<CaptureChunkInput>,
}

impl Writer<'_> {
    async fn push(&mut self, bytes: Vec<u8>) -> Result<(), String> {
        self.refill_candidates().await?;
        let known: Vec<_> = self
            .anchor
            .iter()
            .chain(self.candidates.iter())
            .map(|chunk| KnownChunk {
                id: chunk.id,
                bytes: &chunk.body,
            })
            .collect();
        if let Some(id) = matching_chunk_id(&bytes, &known) {
            self.advance_match(id);
            self.pending.push(CaptureChunkInput::Existing(id));
        } else {
            if self.anchor.is_empty() {
                self.anchor.extend(self.candidates.iter().cloned());
            }
            self.candidates.pop_front();
            self.pending.push(CaptureChunkInput::New(bytes));
        }
        if self.pending.len() == WRITE_BATCH {
            self.flush().await?;
        }
        Ok(())
    }

    fn advance_match(&mut self, id: i64) {
        if let Some(ordinal) = self
            .anchor
            .iter()
            .chain(self.candidates.iter())
            .find(|chunk| chunk.id == id)
            .map(|chunk| chunk.ordinal)
        {
            if self
                .candidates
                .front()
                .is_some_and(|chunk| chunk.ordinal > ordinal)
            {
                self.candidates.clear();
                self.next_candidate = ordinal + 1;
            } else {
                while self
                    .candidates
                    .front()
                    .is_some_and(|chunk| chunk.ordinal <= ordinal)
                {
                    self.candidates.pop_front();
                }
            }
        }
        self.anchor.clear();
    }

    async fn refill_candidates(&mut self) -> Result<(), String> {
        let Some(previous) = self.predecessor else {
            return Ok(());
        };
        if self.candidates.len() >= CANDIDATES as usize / 2 {
            return Ok(());
        }
        let rows = self
            .persist
            .runtime()
            .capture_document_chunks(
                previous,
                self.next_candidate,
                CANDIDATES - self.candidates.len() as i64,
            )
            .await
            .map_err(|error| error.to_string())?;
        self.next_candidate += rows.len() as i64;
        self.candidates.extend(rows);
        Ok(())
    }

    async fn flush(&mut self) -> Result<(), String> {
        if self.pending.is_empty() {
            return Ok(());
        }
        self.persist
            .runtime()
            .append_capture_chunks(self.document, self.ordinal, &self.pending)
            .await
            .map_err(|error| error.to_string())?;
        self.ordinal += self.pending.len() as i64;
        self.pending.clear();
        Ok(())
    }
}

async fn document(
    persist: &PersistHandle,
    id: i64,
    predecessor: Option<i64>,
    path: &Path,
    complete: bool,
) -> Result<(), String> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|error| error.to_string())?;
    let mut chunker = Chunker::default();
    let mut buffer = vec![0; mike_codex::capture::MAX_CHUNK_BYTES];
    let mut writer = Writer {
        persist,
        document: id,
        predecessor,
        candidates: VecDeque::new(),
        anchor: Vec::new(),
        next_candidate: 0,
        ordinal: 0,
        pending: Vec::with_capacity(WRITE_BATCH),
    };
    loop {
        let read = file
            .read(&mut buffer)
            .await
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        for chunk in chunker.push(&buffer[..read]) {
            writer.push(chunk).await?;
        }
    }
    if let Some(tail) = chunker.finish() {
        writer.push(tail).await?;
    }
    writer.flush().await?;
    persist
        .runtime()
        .finish_capture_document(id, complete)
        .await
        .map_err(|error| error.to_string())
}

async fn decode(directory: &Path, name: &str, manifest: &mut Manifest) -> Result<PathBuf, String> {
    let original = directory.join(name);
    let key = format!("{name}_body_encoding");
    let encoding = manifest
        .exchange
        .metadata
        .get(&key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or("identity")
        .to_owned();
    if encoding.is_empty() || encoding.eq_ignore_ascii_case("identity") {
        return Ok(original);
    }
    let locked = if name == "request" {
        manifest.exchange.request_document_id != 0
    } else {
        manifest.exchange.response_document_id.is_some()
    };
    let stored = manifest
        .exchange
        .metadata
        .get(format!("{name}_stored_encoding"))
        .and_then(serde_json::Value::as_str);
    if locked && stored != Some("identity") {
        return Ok(original);
    }
    let decoded = directory.join(format!("{name}.decoded"));
    let result = async {
        let input = tokio::fs::File::open(&original).await?;
        let output = tokio::fs::File::create(&decoded).await?;
        mike_codex::capture::decode_body(&encoding, input, output).await
    }
    .await;
    match result {
        Ok(_) => {
            manifest.exchange.metadata[format!("{name}_stored_encoding")] =
                serde_json::json!("identity");
            if name == "request" {
                manifest.exchange.metadata["sent_stored_encoding"] = serde_json::json!("identity");
            }
            Ok(decoded)
        }
        Err(error) => decode_failed(directory, name, manifest, error, locked).await,
    }
}

async fn decode_failed(
    directory: &Path,
    name: &str,
    manifest: &mut Manifest,
    error: mike_codex::capture::DecodeError,
    locked: bool,
) -> Result<PathBuf, String> {
    if locked {
        return Err(format!(
            "recreating the decoded {name} body failed: {error}"
        ));
    }
    manifest.exchange.metadata[format!("{name}_decoding_error")] =
        serde_json::json!(error.to_string());
    tracing::warn!(%error, capture = %manifest.exchange.id, body = name, "Codex capture keeps original encoded bytes after decoding failed");
    remove_partial_decoded(&directory.join(format!("{name}.decoded"))).await;
    Ok(directory.join(name))
}

async fn remove_partial_decoded(path: &Path) {
    if let Err(error) = tokio::fs::remove_file(path).await
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(%error, "removing incomplete decoded capture failed");
    }
}

#[cfg(test)]
#[path = "capture_write_tests.rs"]
mod tests;
