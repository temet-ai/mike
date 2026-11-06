use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    String(String),
    Integer(i64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcScope {
    pub request_id: RequestId,
    pub method: String,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReceiptId {
    pub namespace_id: Uuid,
    pub ingestion_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceOrigin {
    LiveFrame,
    RpcResponse,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub receipt: ReceiptId,
    pub json_pointer: String,
    pub origin: EvidenceOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Field<T> {
    Missing,
    Null,
    Value(T),
    Invalid(Value),
}

impl<T> Field<T> {
    pub fn value(&self) -> Option<&T> {
        match self {
            Self::Value(value) => Some(value),
            _ => None,
        }
    }
}

pub(super) fn field<T: DeserializeOwned>(value: &Value, key: &str) -> Field<T> {
    match value.get(key) {
        None => Field::Missing,
        Some(Value::Null) => Field::Null,
        Some(value) => match serde_json::from_value(value.clone()) {
            Ok(parsed) => Field::Value(parsed),
            Err(_) => Field::Invalid(value.clone()),
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IssueKind {
    MalformedJson,
    MalformedEnvelope,
    InvalidObservation,
    InvalidIdentifier,
    ConflictingAssociation,
    MissingField,
    InvalidField,
    UnknownMethod,
    UnknownItemType,
    UncorrelatedResponse,
    UnreviewedResponse,
    UnclassifiedTraffic,
    RpcError,
    ExcludedAuthentication,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionIssue {
    pub kind: IssueKind,
    pub json_pointer: String,
}

impl ProjectionIssue {
    pub(super) fn prefixed(mut self, prefix: &str) -> Self {
        self.json_pointer = format!("{prefix}{}", self.json_pointer);
        self
    }

    pub(super) fn new(kind: IssueKind, pointer: impl Into<String>) -> Self {
        Self {
            kind,
            json_pointer: pointer.into(),
        }
    }
}

pub(super) fn identifier(value: &Value, pointer: &str) -> Result<String, ProjectionIssue> {
    let Some(text) = value.pointer(pointer).and_then(Value::as_str) else {
        return Err(ProjectionIssue::new(IssueKind::InvalidIdentifier, pointer));
    };
    if text.trim().is_empty() || text.contains('\0') {
        return Err(ProjectionIssue::new(IssueKind::InvalidIdentifier, pointer));
    }
    Ok(text.to_owned())
}

pub(super) fn authenticated_method(method: &str) -> bool {
    method.starts_with("account/login/")
        || method.starts_with("account/chatgptAuthTokens/")
        || method.starts_with("account/gatewayOAuth/")
        || method.starts_with("mcpServer/oauth")
        || method.starts_with("modelProvider/authRecovery")
        || matches!(
            method,
            "account/read" | "account/logout" | "account/updated" | "attestation/generate"
        )
}

pub(super) fn response_scope_matches(value: &Value, scope: &RpcScope) -> bool {
    value
        .get("id")
        .and_then(|id| serde_json::from_value::<RequestId>(id.clone()).ok())
        .as_ref()
        == Some(&scope.request_id)
}

pub(super) fn reviewed_response_method(method: &str) -> bool {
    matches!(
        method,
        "thread/read" | "thread/items/list" | "thread/turns/list" | "account/usage/read"
    )
}

#[derive(Default)]
pub(super) struct RoutingAudit {
    pub duplicate: bool,
    pub authentication: bool,
    pub method_present: bool,
    pub response_bearing: bool,
}

pub(super) fn audit_routing(bytes: &[u8]) -> RoutingAudit {
    let mut audit = RoutingAudit::default();
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let visitor = RoutingKeyVisitor { audit: &mut audit };
    let _ = serde::de::Deserializer::deserialize_map(&mut deserializer, visitor);
    audit
}

struct RoutingKeyVisitor<'a> {
    audit: &'a mut RoutingAudit,
}

impl<'de> serde::de::Visitor<'de> for RoutingKeyVisitor<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a JSON-RPC object")
    }

    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut seen = std::collections::BTreeSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if matches!(key.as_str(), "method" | "id" | "result" | "error") {
                self.audit.duplicate |= !seen.insert(key.clone());
            }
            self.audit.method_present |= key == "method";
            self.audit.response_bearing |= matches!(key.as_str(), "result" | "error");
            let value = map.next_value::<Value>()?;
            if key == "method" {
                self.audit.authentication |= value.as_str().is_some_and(authenticated_method);
            }
        }
        Ok(())
    }
}

pub(super) fn optional_identifier(value: &Value, key: &str) -> Field<String> {
    match field::<String>(value, key) {
        Field::Value(text) if text.trim().is_empty() || text.contains('\0') => {
            Field::Invalid(Value::String(text))
        }
        value => value,
    }
}
