use super::{SettingApplies, SettingKind, SettingMeta, SettingScope};

pub fn entries() -> Vec<SettingMeta> {
    vec![
        share_entry(
            "llm.budget.steerer.share_pct",
            "Budget share: steerer (%)",
            25,
            "The share of every limit above that is guaranteed to the steerer, as a percentage: the per-minute rate limit, the call ceiling and the daily spend ceiling are each split by these shares. Inside its share the steerer is never refused because another worker used more than its own; past its share it may only draw on the pool that no worker has been guaranteed, which is whatever the seven shares leave out of 100. Shares that add up to more than 100 are scaled down proportionally, so the limits as a whole are never exceeded.",
        ),
        share_entry(
            "llm.budget.session_judge.share_pct",
            "Budget share: session judge (%)",
            20,
            "The share of every limit above that is guaranteed to the session judge, as a percentage: the per-minute rate limit, the call ceiling and the daily spend ceiling are each split by these shares. Inside its share the session judge is never refused because another worker used more than its own; past its share it may only draw on the pool that no worker has been guaranteed, which is whatever the seven shares leave out of 100. Shares that add up to more than 100 are scaled down proportionally, so the limits as a whole are never exceeded.",
        ),
        share_entry(
            "llm.budget.judge_worker.share_pct",
            "Budget share: turn judge (%)",
            25,
            "The share of every limit above that is guaranteed to the turn judge, as a percentage: the per-minute rate limit, the call ceiling and the daily spend ceiling are each split by these shares. Inside its share the turn judge is never refused because another worker used more than its own; past its share it may only draw on the pool that no worker has been guaranteed, which is whatever the seven shares leave out of 100. Shares that add up to more than 100 are scaled down proportionally, so the limits as a whole are never exceeded.",
        ),
        share_entry(
            "llm.budget.lesson_relations.share_pct",
            "Budget share: lesson relations (%)",
            5,
            "The share of every limit above that is guaranteed to the lesson-relations detector, as a percentage: the per-minute rate limit, the call ceiling and the daily spend ceiling are each split by these shares. Inside its share the detector is never refused because another worker used more than its own; past its share it may only draw on the pool that no worker has been guaranteed, which is whatever the seven shares leave out of 100. Shares that add up to more than 100 are scaled down proportionally, so the limits as a whole are never exceeded.",
        ),
        share_entry(
            "llm.budget.lesson_proposer.share_pct",
            "Budget share: lesson proposer (%)",
            10,
            "Guaranteed share of the rate, call and daily spend ceilings for learning from corrections. Unreserved capacity remains shared; shares above 100 total are scaled proportionally. The global daily spend ceiling still applies.",
        ),
        share_entry(
            "llm.budget.eval.share_pct",
            "Budget share: evaluation (%)",
            0,
            "The share of every limit above that is guaranteed to evaluation calls: the offline mike eval runs and the turn guard's shadow labeller. At 0 the evaluation role draws only on the pool that no worker has been guaranteed, which is whatever the seven shares leave out of 100, so it can never take a production worker's share. Raise it for a measurement window and put it back afterwards. Shares that add up to more than 100 are scaled down proportionally, so the limits as a whole are never exceeded.",
        ),
        share_entry(
            "llm.budget.other.share_pct",
            "Budget share: other callers (%)",
            10,
            "The share of every limit above that is guaranteed to every other caller taken together: operator-invoked assists and dry-runs, the correction classifier, and anything not tagged with a worker role. Inside this share those callers are never refused because a worker used more than its own; past it they may only draw on the pool that no worker has been guaranteed, which is whatever the seven shares leave out of 100. Shares that add up to more than 100 are scaled down proportionally, so the limits as a whole are never exceeded.",
        ),
    ]
}

fn share_entry(
    key: &'static str,
    label: &'static str,
    default_pct: i64,
    description: &'static str,
) -> SettingMeta {
    SettingMeta {
        key,
        group: "LLM budget",
        kind: SettingKind::Int,
        scope: SettingScope::Global,
        applies: SettingApplies::NextCycle,
        default: serde_json::Value::Number(serde_json::Number::from(default_pct)),
        label,
        description,
        min_i64: Some(0),
        max_i64: Some(100),
        min_f64: None,
        max_f64: None,
        max_len: None,
        enum_variants: None,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn proposer_share_metadata_default_and_overlay_agree() {
        let key = "llm.budget.lesson_proposer.share_pct";
        let metadata = super::entries().into_iter().find(|entry| entry.key == key);
        assert_eq!(
            metadata.map(|entry| entry.default),
            Some(serde_json::json!(10))
        );
        let default = crate::settings::SettingsOverlay::default();
        assert_eq!(default.llm_budget_lesson_proposer_share_pct, 10);
        let overlay = crate::settings::apply_db_rows_to_overlay(vec![(
            key.to_owned(),
            serde_json::json!(15),
        )]);
        assert_eq!(overlay.llm_budget_lesson_proposer_share_pct, 15);
    }
}
