use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::protocol::{Evidence, Field, IssueKind, ProjectionIssue, RpcScope, field, identifier};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenBreakdown {
    pub input_tokens: Field<i64>,
    pub cached_input_tokens: Field<i64>,
    pub cache_write_input_tokens: Field<i64>,
    pub output_tokens: Field<i64>,
    pub reasoning_output_tokens: Field<i64>,
    pub total_tokens: Field<i64>,
}

impl TokenBreakdown {
    fn read(value: &Value, key: &str) -> Field<Self> {
        let Some(value) = value.get(key) else {
            return Field::Missing;
        };
        if value.is_null() {
            return Field::Null;
        }
        if !value.is_object() {
            return Field::Invalid(value.clone());
        }
        Field::Value(Self {
            input_tokens: field(value, "inputTokens"),
            cached_input_tokens: field(value, "cachedInputTokens"),
            cache_write_input_tokens: field(value, "cacheWriteInputTokens"),
            output_tokens: field(value, "outputTokens"),
            reasoning_output_tokens: field(value, "reasoningOutputTokens"),
            total_tokens: field(value, "totalTokens"),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageObservation {
    pub thread_id: String,
    pub turn_id: String,
    pub evidence: Evidence,
    pub total: Field<TokenBreakdown>,
    pub last: Field<TokenBreakdown>,
    pub context_window: Field<i64>,
    pub provider_payload: Value,
}

pub(super) fn decode_usage(
    value: &Value,
    evidence: Evidence,
) -> Result<UsageObservation, ProjectionIssue> {
    let thread_id =
        identifier(value, "/threadId").map_err(|issue| issue.prefixed(&evidence.json_pointer))?;
    let turn_id =
        identifier(value, "/turnId").map_err(|issue| issue.prefixed(&evidence.json_pointer))?;
    let usage = value
        .get("tokenUsage")
        .filter(|usage| usage.is_object())
        .ok_or_else(|| ProjectionIssue::new(IssueKind::InvalidField, "/params/tokenUsage"))?;
    Ok(UsageObservation {
        thread_id,
        turn_id,
        evidence,
        total: TokenBreakdown::read(usage, "total"),
        last: TokenBreakdown::read(usage, "last"),
        context_window: field(usage, "modelContextWindow"),
        provider_payload: usage.clone(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EstimateAvailability {
    Missing,
    Unavailable,
    Reported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BillingEstimate {
    pub thread_id: String,
    pub evidence: Evidence,
    pub availability: EstimateAvailability,
    pub estimated_credits_micros: Field<i64>,
    pub estimated_usd_micros: Field<i64>,
    pub groups: Field<Vec<Value>>,
    pub provider_payload: Value,
}

pub(super) fn decode_billing(
    value: &Value,
    scope: &RpcScope,
    evidence: Evidence,
) -> Result<BillingEstimate, ProjectionIssue> {
    let requested = scope
        .thread_id
        .as_ref()
        .ok_or_else(|| ProjectionIssue::new(IssueKind::MissingField, "/request/threadId"))?;
    identifier(&serde_json::json!({"threadId": requested}), "/threadId")
        .map_err(|issue| issue.prefixed("/request"))?;
    let (usage, availability) = match value.get("threadUsage") {
        None => (&Value::Null, EstimateAvailability::Missing),
        Some(Value::Null) => (&Value::Null, EstimateAvailability::Unavailable),
        Some(usage) => (usage, EstimateAvailability::Reported),
    };
    if availability == EstimateAvailability::Reported
        && identifier(usage, "/threadId")
            .map_err(|issue| issue.prefixed(&format!("{}/threadUsage", evidence.json_pointer)))?
            != *requested
    {
        return Err(ProjectionIssue::new(
            IssueKind::ConflictingAssociation,
            "/result/threadUsage/threadId",
        ));
    }
    Ok(BillingEstimate {
        thread_id: requested.clone(),
        evidence,
        availability,
        estimated_credits_micros: field(usage, "estimatedUsageCreditsMicros"),
        estimated_usd_micros: field(usage, "estimatedUsageUsdMicros"),
        groups: field(usage, "groups"),
        provider_payload: usage.clone(),
    })
}
