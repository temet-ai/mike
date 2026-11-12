use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::project::ProjectionEffect;
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadSpend {
    pub thread_id: String,
    pub cumulative: i64,
    pub inherited: i64,
    pub own: i64,
    pub samples: usize,
    pub unreported_samples: usize,
    pub decreases: usize,
}

#[derive(Debug, Clone, Default)]
pub struct UsageLedger {
    samples: BTreeMap<String, Vec<(String, Option<i64>)>>,
    sources: BTreeMap<String, String>,
}

impl UsageLedger {
    pub fn apply(&mut self, effect: &ProjectionEffect) {
        match effect {
            ProjectionEffect::Usage(usage) => self.record(usage),
            ProjectionEffect::Lifecycle(lifecycle) => {
                let source = lifecycle
                    .thread
                    .as_ref()
                    .and_then(|thread| thread.forked_from_id.value());
                if let Some(source) = source {
                    self.inherits_from(&lifecycle.thread_id, source);
                }
            }
            _ => {}
        }
    }

    pub fn record(&mut self, usage: &UsageObservation) {
        let total = usage
            .total
            .value()
            .and_then(|total| total.total_tokens.value().copied());
        self.samples
            .entry(usage.thread_id.clone())
            .or_default()
            .push((usage.turn_id.clone(), total));
    }

    pub fn inherits_from(&mut self, thread_id: &str, source_thread_id: &str) {
        if thread_id != source_thread_id {
            self.sources
                .insert(thread_id.to_owned(), source_thread_id.to_owned());
        }
    }

    pub fn spend(&self) -> Vec<ThreadSpend> {
        self.samples
            .keys()
            .map(|thread| self.thread_spend(thread))
            .collect()
    }

    pub fn own_total(&self) -> i64 {
        self.spend().iter().map(|spend| spend.own).sum()
    }

    fn thread_spend(&self, thread: &str) -> ThreadSpend {
        let samples = self.samples.get(thread).map(Vec::as_slice).unwrap_or(&[]);
        let inherited_turns = self.ancestor_turns(thread);
        let reported: Vec<(&str, i64)> = samples
            .iter()
            .filter_map(|(turn, total)| total.map(|total| (turn.as_str(), total)))
            .collect();
        let cumulative = reported.iter().map(|(_, total)| *total).max().unwrap_or(0);
        let inherited = reported
            .iter()
            .filter(|(turn, _)| inherited_turns.contains(*turn))
            .map(|(_, total)| *total)
            .max()
            .unwrap_or(0);
        ThreadSpend {
            thread_id: thread.to_owned(),
            cumulative,
            inherited,
            own: cumulative.saturating_sub(inherited),
            samples: samples.len(),
            unreported_samples: samples.len() - reported.len(),
            decreases: reported.windows(2).filter(|w| w[1].1 < w[0].1).count(),
        }
    }

    fn ancestor_turns(&self, thread: &str) -> BTreeSet<&str> {
        let mut turns = BTreeSet::new();
        let mut seen = BTreeSet::from([thread]);
        let mut current = thread;
        while let Some(source) = self.sources.get(current) {
            if !seen.insert(source.as_str()) {
                break;
            }
            let own_turns = self.samples.get(source).into_iter().flatten();
            turns.extend(own_turns.map(|(turn, _)| turn.as_str()));
            current = source;
        }
        turns
    }
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
