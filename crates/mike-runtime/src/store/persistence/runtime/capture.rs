use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::store::persistence::PersistError;

#[path = "capture/documents.rs"]
mod documents;
#[path = "capture/exchanges.rs"]
mod exchanges;
#[path = "capture/reads.rs"]
mod reads;
#[cfg(test)]
#[path = "capture/tests.rs"]
mod tests;

#[derive(Debug, Clone)]
pub enum CaptureChunkInput {
    New(Vec<u8>),
    Existing(i64),
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct KnownCaptureChunk {
    pub id: i64,
    pub ordinal: i64,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct CaptureDocumentRow {
    pub id: i64,
    pub byte_size: i64,
    pub chunk_count: i64,
    pub finished: bool,
    pub complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CaptureExchangeInput {
    pub id: Uuid,
    pub session_uuid: Option<Uuid>,
    pub provider_thread_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub metadata: Value,
    pub request_document_id: i64,
    pub sent_document_id: i64,
    pub response_document_id: Option<i64>,
    pub status: String,
}

pub type CaptureExchangeRow = CaptureExchangeInput;

impl crate::store::persistence::runtime_repo::RuntimeDb<'_> {
    pub async fn create_capture_document(
        self,
        predecessor: Option<i64>,
    ) -> Result<i64, PersistError> {
        documents::create(self.pool(), predecessor).await
    }

    pub async fn append_capture_chunks(
        self,
        document: i64,
        start_ordinal: i64,
        chunks: &[CaptureChunkInput],
    ) -> Result<Vec<i64>, PersistError> {
        documents::append(self.pool(), document, start_ordinal, chunks).await
    }

    pub async fn finish_capture_document(
        self,
        document: i64,
        complete: bool,
    ) -> Result<(), PersistError> {
        documents::finish(self.pool(), document, complete).await
    }

    pub async fn capture_document(
        self,
        document: i64,
    ) -> Result<Option<CaptureDocumentRow>, PersistError> {
        reads::document(self.pool(), document).await
    }

    pub async fn capture_document_chunks(
        self,
        document: i64,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<KnownCaptureChunk>, PersistError> {
        reads::chunks(self.pool(), document, offset, limit).await
    }

    pub async fn upsert_capture_exchange(
        self,
        input: &CaptureExchangeInput,
    ) -> Result<(), PersistError> {
        exchanges::upsert(self.pool(), input).await
    }

    pub async fn get_capture_exchange(
        self,
        id: Uuid,
    ) -> Result<Option<CaptureExchangeRow>, PersistError> {
        reads::exchange(self.pool(), id).await
    }

    pub async fn list_capture_exchanges(
        self,
        session: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<CaptureExchangeRow>, PersistError> {
        reads::exchanges(self.pool(), session, limit, offset).await
    }

    pub async fn previous_capture_request(
        self,
        session: Option<Uuid>,
        thread: Option<&str>,
    ) -> Result<Option<i64>, PersistError> {
        reads::previous(self.pool(), session, thread).await
    }
}

fn invalid(message: &str) -> PersistError {
    PersistError::DataInvariant(message.into())
}
