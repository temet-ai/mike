use serde_json::Value;

use super::audit::{HistoryAudit, audit};
use super::entries::{check_optional_scope, item, metadata, turn};
use super::{
    HistoryEntry, HistoryError, HistoryErrorKind, HistoryMetadata, HistoryRequest, HistoryScope,
};
use crate::runtime::RuntimeObservation;
use crate::runtime::codex::protocol::{field, response_scope_matches};
use crate::runtime::codex::{Evidence, EvidenceOrigin, Field, ReceiptId};

#[derive(Debug)]
pub struct HistoryPage<'a> {
    observation: &'a RuntimeObservation,
    request: &'a HistoryRequest,
    result: &'a Value,
    entries: &'a [Value],
    metadata: Option<HistoryMetadata<'a>>,
    next_cursor: Field<String>,
    backwards_cursor: Field<String>,
    entry_errors: std::collections::BTreeMap<usize, HistoryError>,
}

impl<'a> HistoryPage<'a> {
    pub fn decode(
        observation: &'a RuntimeObservation,
        request: &'a HistoryRequest,
    ) -> Result<Self, HistoryError> {
        observation
            .validate()
            .map_err(|_| HistoryError::new(HistoryErrorKind::InvalidObservation, ""))?;
        check_envelope(observation, request)?;
        let HistoryAudit {
            control_error,
            entry_errors,
        } = audit(
            &observation.raw_payload,
            matches!(request.query().scope(), HistoryScope::Items { .. }),
        )?;
        if let Some(error) = control_error {
            return Err(error);
        }
        let result = response_result(observation, request)?;
        let mut page = Self {
            observation,
            request,
            result,
            entries: &[],
            metadata: None,
            next_cursor: field(result, "nextCursor"),
            backwards_cursor: field(result, "backwardsCursor"),
            entry_errors,
        };
        page.decode_contents()?;
        Ok(page)
    }

    fn decode_contents(&mut self) -> Result<(), HistoryError> {
        if self.request.query().scope() == &HistoryScope::Metadata {
            let value = self.result.get("thread").ok_or_else(|| {
                HistoryError::new(HistoryErrorKind::InvalidData, "/result/thread")
            })?;
            self.metadata = Some(metadata(
                value,
                self.request,
                self.evidence("/result/thread"),
            )?);
        } else {
            self.entries = self
                .result
                .get("data")
                .and_then(Value::as_array)
                .ok_or_else(|| HistoryError::new(HistoryErrorKind::InvalidData, "/result/data"))?;
        }
        Ok(())
    }

    pub fn request(&self) -> &HistoryRequest {
        self.request
    }

    pub fn raw_payload(&self) -> &[u8] {
        &self.observation.raw_payload
    }

    pub fn provider_result(&self) -> &Value {
        self.result
    }

    pub fn metadata(&self) -> Option<&HistoryMetadata<'a>> {
        self.metadata.as_ref()
    }

    pub fn next_cursor(&self) -> &Field<String> {
        &self.next_cursor
    }

    pub fn backwards_cursor(&self) -> &Field<String> {
        &self.backwards_cursor
    }

    pub fn entries(&self) -> impl Iterator<Item = Result<HistoryEntry<'a>, HistoryError>> + '_ {
        self.entries
            .iter()
            .enumerate()
            .map(|(index, value)| self.decode_entry(index, value))
    }

    pub fn evidence(&self, pointer: &str) -> Evidence {
        Evidence {
            receipt: ReceiptId {
                namespace_id: self.observation.envelope.namespace_id,
                ingestion_id: self.observation.envelope.ingestion_id,
            },
            json_pointer: pointer.into(),
            origin: EvidenceOrigin::RpcResponse,
        }
    }

    fn decode_entry(
        &self,
        index: usize,
        value: &'a Value,
    ) -> Result<HistoryEntry<'a>, HistoryError> {
        if let Some(error) = self.entry_errors.get(&index) {
            return Err(error.clone());
        }
        let evidence = self.evidence(&format!("/result/data/{index}"));
        if self.request.query().scope() == &HistoryScope::Turns {
            return turn(value, self.request, evidence);
        }
        let entry = item(value, self.request, evidence)?;
        self.check_item_envelope(&entry)?;
        Ok(entry)
    }

    fn check_item_envelope(&self, entry: &HistoryEntry<'_>) -> Result<(), HistoryError> {
        let HistoryEntry::Item(item) = entry else {
            return Ok(());
        };
        if self
            .observation
            .envelope
            .provider_item_id
            .as_deref()
            .is_some_and(|id| id != item.item_id)
        {
            return Err(HistoryError::new(
                HistoryErrorKind::ConflictingScope,
                format!("{}/id", item.evidence.json_pointer),
            ));
        }
        Ok(())
    }
}

fn check_envelope(
    observation: &RuntimeObservation,
    request: &HistoryRequest,
) -> Result<(), HistoryError> {
    let envelope = &observation.envelope;
    let query = request.query();
    let wrong_namespace = envelope.namespace_id != query.identity().namespace_id();
    let wrong_thread = envelope
        .provider_thread_id
        .as_deref()
        .is_some_and(|id| id != query.identity().thread_id());
    let wrong_turn = envelope
        .provider_turn_id
        .as_deref()
        .is_some_and(|id| Some(id) != query.turn_id());
    let unexpected_item = envelope.provider_item_id.is_some() && query.turn_id().is_none();
    if wrong_namespace || wrong_thread || wrong_turn || unexpected_item {
        return Err(HistoryError::new(
            HistoryErrorKind::ConflictingScope,
            "/envelope",
        ));
    }
    Ok(())
}

fn response_result<'a>(
    observation: &'a RuntimeObservation,
    request: &HistoryRequest,
) -> Result<&'a Value, HistoryError> {
    let value = observation
        .parsed_payload
        .as_ref()
        .filter(|value| value.is_object())
        .ok_or_else(|| HistoryError::new(HistoryErrorKind::InvalidEnvelope, ""))?;
    if value.get("method").is_some()
        || (value.get("error").is_some() && value.get("result").is_some())
    {
        return Err(HistoryError::new(HistoryErrorKind::InvalidEnvelope, ""));
    }
    if !response_scope_matches(value, &request.rpc_scope()) {
        return Err(HistoryError::new(
            HistoryErrorKind::UncorrelatedResponse,
            "/id",
        ));
    }
    if value.get("error").is_some() {
        return Err(HistoryError::new(HistoryErrorKind::RpcError, "/error"));
    }
    let result = value
        .get("result")
        .filter(|result| result.is_object())
        .ok_or_else(|| HistoryError::new(HistoryErrorKind::InvalidData, "/result"))?;
    check_optional_scope(
        result,
        "threadId",
        Some(request.query().identity().thread_id()),
        "/result",
    )?;
    check_optional_scope(result, "turnId", request.query().turn_id(), "/result")?;
    Ok(result)
}
