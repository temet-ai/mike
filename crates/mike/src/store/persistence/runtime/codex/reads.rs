use sqlx::PgExecutor;
use uuid::Uuid;

use crate::runtime::RuntimeObservation;
use crate::runtime::codex::ReceiptId;
use crate::store::persistence::PersistError;

use super::super::invalid;
use super::context::CONTEXT_VERSION;
use super::{CodexIntakeContext, RawEventPage, StoredRuntimeReceipt, invariant};

pub(super) struct ReceiptRow {
    pub(super) event_id: i64,
    pub(super) namespace_id: Uuid,
    pub(super) ingestion_id: Uuid,
    pub(super) envelope: String,
    pub(super) raw_payload: Vec<u8>,
    pub(super) parsed_payload: Option<String>,
    pub(super) capture_sequence: Option<i64>,
    pub(super) context_version: Option<i32>,
    pub(super) connection_epoch: Option<Uuid>,
    pub(super) frame_ordinal: Option<i64>,
    pub(super) transport_gap: Option<bool>,
    pub(super) context_document: Option<String>,
}

impl TryFrom<ReceiptRow> for StoredRuntimeReceipt {
    type Error = PersistError;

    fn try_from(row: ReceiptRow) -> Result<Self, Self::Error> {
        let context: Option<CodexIntakeContext> = row
            .context_document
            .as_deref()
            .map(serde_json::from_str)
            .transpose()?;
        validate_context_columns(&row, context.as_ref())?;
        let observation = RuntimeObservation {
            envelope: serde_json::from_str(&row.envelope)?,
            raw_payload: row.raw_payload,
            parsed_payload: row
                .parsed_payload
                .as_deref()
                .map(serde_json::from_str)
                .transpose()?,
        };
        observation.validate().map_err(invalid)?;
        if observation.envelope.namespace_id != row.namespace_id
            || observation.envelope.ingestion_id != row.ingestion_id
        {
            return Err(invariant(
                "stored raw envelope does not match its receipt identity",
            ));
        }
        Ok(Self {
            event_id: row.event_id,
            observation,
            capture_sequence: row.capture_sequence,
            context,
            transport_gap: row.transport_gap,
        })
    }
}

fn validate_context_columns(
    row: &ReceiptRow,
    context: Option<&CodexIntakeContext>,
) -> Result<(), PersistError> {
    let Some(context) = context else {
        if row.capture_sequence.is_some() || row.context_version.is_some() {
            return Err(invariant("partial trusted receipt context"));
        }
        return Ok(());
    };
    context.validate()?;
    if row.context_version != Some(CONTEXT_VERSION)
        || row.connection_epoch != Some(context.connection_epoch())
        || row.frame_ordinal != Some(context.frame_ordinal())
        || row.capture_sequence.is_none_or(|sequence| sequence <= 0)
        || row.transport_gap.is_none()
    {
        return Err(invariant(
            "trusted context document disagrees with indexed context",
        ));
    }
    Ok(())
}

pub(super) async fn receipt_by_ingestion<'e>(
    executor: impl PgExecutor<'e>,
    receipt: ReceiptId,
) -> Result<Option<StoredRuntimeReceipt>, PersistError> {
    let row = sqlx::query_file_as!(
        ReceiptRow,
        "src/store/persistence/runtime/codex/receipt_by_ingestion.sql",
        receipt.namespace_id,
        receipt.ingestion_id,
    )
    .fetch_optional(executor)
    .await?;
    row.map(TryInto::try_into).transpose()
}

impl crate::store::persistence::runtime_repo::RuntimeDb<'_> {
    pub async fn runtime_receipt(
        self,
        receipt: ReceiptId,
    ) -> Result<Option<StoredRuntimeReceipt>, PersistError> {
        receipt_by_ingestion(&self.pool, receipt).await
    }

    pub async fn list_runtime_raw_events(
        self,
        page: RawEventPage,
    ) -> Result<Vec<StoredRuntimeReceipt>, PersistError> {
        validate_raw_page(&page)?;
        let rows = sqlx::query_file_as!(
            ReceiptRow,
            "src/store/persistence/runtime/codex/raw_events_page.sql",
            page.namespace_id,
            page.after_event_id,
            page.limit,
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    pub async fn list_codex_unclassified_receipts(
        self,
        page: RawEventPage,
    ) -> Result<Vec<StoredRuntimeReceipt>, PersistError> {
        validate_raw_page(&page)?;
        let rows = sqlx::query_file_as!(
            ReceiptRow,
            "src/store/persistence/runtime/codex/raw_events_unclassified_page.sql",
            page.namespace_id,
            page.after_event_id,
            page.limit,
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(TryInto::try_into).collect()
    }
}

fn validate_raw_page(page: &RawEventPage) -> Result<(), PersistError> {
    validate_page(page.limit)?;
    if page.after_event_id < 0 {
        return Err(invariant("raw event cursor must not be negative"));
    }
    Ok(())
}

const MAX_PAGE_SIZE: i64 = 1000;

pub(super) fn validate_page(limit: i64) -> Result<(), PersistError> {
    if !(1..=MAX_PAGE_SIZE).contains(&limit) {
        return Err(invariant("page size must be between 1 and 1000"));
    }
    Ok(())
}
