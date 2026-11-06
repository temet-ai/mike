use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::Question;
use super::policy::GuardConfig;
use crate::cross_session::CrossSessionSnapshot;
use crate::cross_session::effective::{ThresholdChoice, effective_threshold};

pub fn choose(
    snapshot: &CrossSessionSnapshot,
    cfg: &GuardConfig,
    open: &[Question],
    project_id: Option<i64>,
) -> BTreeMap<Question, ThresholdChoice> {
    open.iter()
        .map(|q| {
            (
                *q,
                effective_threshold(snapshot, q.code(), project_id, cfg.threshold(*q)),
            )
        })
        .collect()
}

pub fn to_json(choices: &BTreeMap<Question, ThresholdChoice>) -> Value {
    Value::Object(
        choices
            .iter()
            .map(|(q, c)| (q.code().to_owned(), json!({"threshold_used": c.value, "threshold_source": c.source, "shadow_threshold": c.shadow_value})))
            .collect(),
    )
}
