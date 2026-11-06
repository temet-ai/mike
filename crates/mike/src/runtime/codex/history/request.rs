use std::num::NonZeroU32;

use serde_json::{Value, json};
use uuid::Uuid;

use super::{HistoryError, HistoryErrorKind};
use crate::runtime::codex::{RequestId, RpcScope};
use crate::runtime::{validate_text, validate_uuid};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryIdentity {
    scan_id: Uuid,
    namespace_id: Uuid,
    thread_id: String,
}

impl HistoryIdentity {
    pub fn new(scan_id: Uuid, namespace_id: Uuid, thread_id: String) -> Result<Self, HistoryError> {
        validate_uuid(scan_id, "scan_id")
            .and_then(|()| validate_uuid(namespace_id, "namespace_id"))
            .and_then(|()| validate_text(&thread_id, "thread_id"))
            .map_err(|_| HistoryError::new(HistoryErrorKind::InvalidIdentity, "/request"))?;
        Ok(Self {
            scan_id,
            namespace_id,
            thread_id,
        })
    }

    pub fn scan_id(&self) -> Uuid {
        self.scan_id
    }

    pub fn namespace_id(&self) -> Uuid {
        self.namespace_id
    }

    pub fn thread_id(&self) -> &str {
        &self.thread_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryScope {
    Metadata,
    Turns,
    Items { turn_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryQuery {
    identity: HistoryIdentity,
    scope: HistoryScope,
    limit: NonZeroU32,
    cursor: Option<String>,
}

impl HistoryQuery {
    pub fn metadata(identity: HistoryIdentity, limit: u32) -> Result<Self, HistoryError> {
        Self::new(identity, HistoryScope::Metadata, limit, None)
    }

    pub fn turns(
        identity: HistoryIdentity,
        limit: u32,
        cursor: Option<String>,
    ) -> Result<Self, HistoryError> {
        Self::new(identity, HistoryScope::Turns, limit, cursor)
    }

    pub fn items(
        identity: HistoryIdentity,
        turn_id: String,
        limit: u32,
        cursor: Option<String>,
    ) -> Result<Self, HistoryError> {
        validate_text(&turn_id, "turn_id")
            .map_err(|_| HistoryError::new(HistoryErrorKind::InvalidIdentity, "/request/turnId"))?;
        Self::new(identity, HistoryScope::Items { turn_id }, limit, cursor)
    }

    fn new(
        identity: HistoryIdentity,
        scope: HistoryScope,
        limit: u32,
        cursor: Option<String>,
    ) -> Result<Self, HistoryError> {
        let limit = NonZeroU32::new(limit)
            .ok_or_else(|| HistoryError::new(HistoryErrorKind::ZeroPageLimit, "/request/limit"))?;
        Ok(Self {
            identity,
            scope,
            limit,
            cursor,
        })
    }

    pub fn identity(&self) -> &HistoryIdentity {
        &self.identity
    }

    pub fn scope(&self) -> &HistoryScope {
        &self.scope
    }

    pub fn limit(&self) -> u32 {
        self.limit.get()
    }

    pub fn cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }

    pub fn method(&self) -> &'static str {
        match self.scope {
            HistoryScope::Metadata => "thread/read",
            HistoryScope::Turns => "thread/turns/list",
            HistoryScope::Items { .. } => "thread/items/list",
        }
    }

    pub fn turn_id(&self) -> Option<&str> {
        match &self.scope {
            HistoryScope::Items { turn_id } => Some(turn_id),
            _ => None,
        }
    }

    pub fn request(self, request_id: RequestId) -> HistoryRequest {
        HistoryRequest {
            query: self,
            request_id,
        }
    }

    pub(super) fn continuation(&self, cursor: String) -> Self {
        Self {
            cursor: Some(cursor),
            ..self.clone()
        }
    }

    pub(super) fn turn_query(&self) -> Self {
        Self {
            scope: HistoryScope::Turns,
            cursor: None,
            ..self.clone()
        }
    }

    fn params(&self) -> Value {
        let thread = &self.identity.thread_id;
        if self.scope == HistoryScope::Metadata {
            return json!({"threadId": thread, "includeTurns": false});
        }
        let mut params = json!({
            "threadId": thread, "cursor": self.cursor,
            "sortDirection": "asc", "limit": self.limit.get()
        });
        match &self.scope {
            HistoryScope::Turns => params["itemsView"] = json!("notLoaded"),
            HistoryScope::Items { turn_id } => params["turnId"] = json!(turn_id),
            HistoryScope::Metadata => {}
        }
        params
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRequest {
    query: HistoryQuery,
    request_id: RequestId,
}

impl HistoryRequest {
    pub fn query(&self) -> &HistoryQuery {
        &self.query
    }

    pub fn request_id(&self) -> &RequestId {
        &self.request_id
    }

    pub fn rpc_scope(&self) -> RpcScope {
        RpcScope {
            request_id: self.request_id.clone(),
            method: self.query.method().into(),
            thread_id: Some(self.query.identity.thread_id.clone()),
            turn_id: self.query.turn_id().map(str::to_owned),
        }
    }

    pub fn wire_value(&self) -> Value {
        json!({
            "id": self.request_id, "method": self.query.method(),
            "params": self.query.params()
        })
    }
}
