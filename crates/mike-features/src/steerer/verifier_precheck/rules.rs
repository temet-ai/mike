use serde_json::{Value, json};

use super::facts::{AskSource, Facts, NudgeSource};
use super::holdback::HOLDBACK_SHARE;
use super::questions::POINT;
use crate::llm::decisions::point::{Asked, Decider};
use crate::llm::decisions::record::{CallResult, DecisionContext, DecisionSubject, RecordInsert};

pub const RULES_SET: &str = "verifier_precheck_rules_v1";
pub const RULES_MODEL: &str = "code_rule";

pub const MAILBOX_NUDGE: &str = "mailbox_nudge";
pub const OVERRIDE_NUDGE: &str = "override_nudge";
pub const EMPTY_ASK: &str = "judged_ask_empty";
pub const COMPACTION_ASK: &str = "judged_ask_compaction";
pub const MACHINE_ASK: &str = "judged_ask_machine_text";

pub fn settle(facts: &Facts) -> Option<&'static str> {
    match facts.nudge_source {
        NudgeSource::Mailbox => return Some(MAILBOX_NUDGE),
        NudgeSource::Other => return Some(OVERRIDE_NUDGE),
        NudgeSource::Verifier | NudgeSource::Criteria => {}
    }
    if facts.judged_ask.trim().is_empty() {
        return Some(EMPTY_ASK);
    }
    match facts.ask_source {
        AskSource::Compaction => Some(COMPACTION_ASK),
        AskSource::Unknown => Some(MACHINE_ASK),
        AskSource::Operator | AskSource::PeerMail | AskSource::Brief => None,
    }
}

pub async fn record(
    decider: &Decider,
    rule: &'static str,
    state: &Value,
    subject: DecisionSubject,
) -> Asked {
    let mut context = DecisionContext::new(POINT, decider.mode, RULES_SET);
    context.subject = subject;
    context.thresholds = json!({ "holdback_share": HOLDBACK_SHARE });
    let request = json!({ "rule": rule });
    let result = CallResult {
        outcome: "ok",
        ..CallResult::default()
    };
    let inserted = crate::llm::decisions::store(decider.persist())
        .insert_decision_record(RecordInsert {
            context: &context,
            model_requested: RULES_MODEL,
            state,
            questions: &json!({}),
            request_body: &request,
            result: &result,
            created_at: None,
        })
        .await;
    match inserted {
        Ok(id) => Asked {
            record_id: Some(id),
            ..Asked::default()
        },
        Err(error) => {
            tracing::warn!(%error, rule, "verifier pre-check: code-rule record failed");
            Asked::default()
        }
    }
}
