use serde_json::Value;

use super::store::AnomalyClassification;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubPattern {
    UnregisteredClass,
    UnregisteredSubagent,
    FilterInducedDrop,
    MissingSystem,
    StringFormSystem,
    NonThreeBlockLayout,
    NonTextBlockAtIndex,
    Block2TextMissing,
    Unknown,
}

impl SubPattern {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::UnregisteredClass => "unregistered-class",
            Self::UnregisteredSubagent => "unregistered-subagent",
            Self::FilterInducedDrop => "filter-induced-drop",
            Self::MissingSystem => "missing-system",
            Self::StringFormSystem => "string-form-system",
            Self::NonThreeBlockLayout => "non-three-block-layout",
            Self::NonTextBlockAtIndex => "non-text-block-at-index",
            Self::Block2TextMissing => "block-2-text-missing",
            Self::Unknown => "unknown",
        }
    }

    pub(crate) fn description(&self) -> &'static str {
        match self {
            Self::UnregisteredClass => {
                "Valid 3-block prompt with no composition registered for its sha."
            }
            Self::UnregisteredSubagent => {
                "Subagent invocation with no composition for its sha — register a variant so the operator's catalogue tracks it."
            }
            Self::FilterInducedDrop => {
                "Filter rule rewrote the system field — the composer is reacting to your own filter."
            }
            Self::MissingSystem => "Request had no system field — caller is misconfigured.",
            Self::StringFormSystem => {
                "system was a JSON string instead of a 3-block array (old API shape)."
            }
            Self::NonThreeBlockLayout => {
                "system was a JSON array but length was not 3 — register a class variant for this layout."
            }
            Self::NonTextBlockAtIndex => {
                "One of the system blocks is not a {type: \"text\"} block."
            }
            Self::Block2TextMissing => {
                "system[2] is missing its `text` field (or it is not a string)."
            }
            Self::Unknown => {
                "Unrecognised shape — please share the sample request with the proxy team."
            }
        }
    }

    pub(crate) fn recommended_action(&self) -> &'static str {
        match self {
            Self::UnregisteredClass => "register-class",
            Self::UnregisteredSubagent => "register-variant",
            Self::FilterInducedDrop => "view-filter-diff",
            Self::MissingSystem => "investigate-caller",
            Self::StringFormSystem => "investigate-caller",
            Self::NonThreeBlockLayout => "register-variant",
            Self::NonTextBlockAtIndex => "register-variant",
            Self::Block2TextMissing => "investigate-caller",
            Self::Unknown => "open-sample-request",
        }
    }
}

pub(crate) fn recognise(
    classification: AnomalyClassification,
    unknown_blocks: Option<&Value>,
    firing_rule_ids: &[String],
    persona: Option<&str>,
) -> SubPattern {
    if !firing_rule_ids.is_empty() {
        return SubPattern::FilterInducedDrop;
    }

    match classification {
        AnomalyClassification::UnknownClass => match persona {
            Some(_) => SubPattern::UnregisteredSubagent,
            None => SubPattern::UnregisteredClass,
        },
        AnomalyClassification::StructuralMismatch => match unknown_blocks {
            Some(blocks) => {
                let reason = blocks.get("reason").and_then(|v| v.as_str());
                match reason {
                    Some("missing") => SubPattern::MissingSystem,
                    Some("string_form") => SubPattern::StringFormSystem,
                    Some("non_3_block") => SubPattern::NonThreeBlockLayout,
                    Some("non_text_block") => SubPattern::NonTextBlockAtIndex,
                    Some("block_2_text_missing") => SubPattern::Block2TextMissing,
                    _ => SubPattern::Unknown,
                }
            }
            None => SubPattern::Unknown,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unknown_class_without_persona_maps_to_unregistered_class() {
        let p = recognise(AnomalyClassification::UnknownClass, None, &[], None);
        assert_eq!(p, SubPattern::UnregisteredClass);
        assert_eq!(p.name(), "unregistered-class");
        assert_eq!(p.recommended_action(), "register-class");
    }

    #[test]
    fn unknown_class_with_persona_maps_to_unregistered_subagent() {
        let p = recognise(
            AnomalyClassification::UnknownClass,
            None,
            &[],
            Some("Frontend Development Expert"),
        );
        assert_eq!(p, SubPattern::UnregisteredSubagent);
        assert_eq!(p.name(), "unregistered-subagent");
        assert_eq!(p.recommended_action(), "register-variant");
    }

    #[test]
    fn missing_system_reason_maps_to_missing_system() {
        let blocks = json!({"reason": "missing"});
        let p = recognise(
            AnomalyClassification::StructuralMismatch,
            Some(&blocks),
            &[],
            None,
        );
        assert_eq!(p, SubPattern::MissingSystem);
        assert_eq!(p.name(), "missing-system");
    }

    #[test]
    fn string_form_reason_maps_to_string_form_system() {
        let blocks = json!({"reason": "string_form"});
        let p = recognise(
            AnomalyClassification::StructuralMismatch,
            Some(&blocks),
            &[],
            None,
        );
        assert_eq!(p, SubPattern::StringFormSystem);
    }

    #[test]
    fn non_3_block_reason_maps_to_non_three_block_layout() {
        let blocks = json!({"reason": "non_3_block", "len": 2});
        let p = recognise(
            AnomalyClassification::StructuralMismatch,
            Some(&blocks),
            &[],
            None,
        );
        assert_eq!(p, SubPattern::NonThreeBlockLayout);
        assert_eq!(p.recommended_action(), "register-variant");
    }

    #[test]
    fn non_text_block_reason_maps_to_non_text_block_at_index() {
        let blocks = json!({"reason": "non_text_block", "index": 1});
        let p = recognise(
            AnomalyClassification::StructuralMismatch,
            Some(&blocks),
            &[],
            None,
        );
        assert_eq!(p, SubPattern::NonTextBlockAtIndex);
    }

    #[test]
    fn block_2_text_missing_reason_maps_to_block_2_text_missing() {
        let blocks = json!({"reason": "block_2_text_missing"});
        let p = recognise(
            AnomalyClassification::StructuralMismatch,
            Some(&blocks),
            &[],
            None,
        );
        assert_eq!(p, SubPattern::Block2TextMissing);
    }

    #[test]
    fn unrecognised_reason_falls_through_to_unknown() {
        let blocks = json!({"reason": "future-reason-not-yet-defined"});
        let p = recognise(
            AnomalyClassification::StructuralMismatch,
            Some(&blocks),
            &[],
            None,
        );
        assert_eq!(p, SubPattern::Unknown);
    }

    #[test]
    fn filter_modified_wins_over_structural_reason() {
        let blocks = json!({"reason": "non_3_block", "len": 2});
        let p = recognise(
            AnomalyClassification::StructuralMismatch,
            Some(&blocks),
            &["rule-1".to_string()],
            None,
        );
        assert_eq!(
            p,
            SubPattern::FilterInducedDrop,
            "a fired filter rule must dominate the reason-string mapping"
        );
    }

    #[test]
    fn filter_modified_with_unknown_class_still_maps_to_filter_induced_drop() {
        let p = recognise(
            AnomalyClassification::UnknownClass,
            None,
            &["rule-1".to_string()],
            None,
        );
        assert_eq!(p, SubPattern::FilterInducedDrop);
    }

    #[test]
    fn filter_modified_wins_over_persona() {
        let p = recognise(
            AnomalyClassification::UnknownClass,
            None,
            &["rule-1".to_string()],
            Some("Frontend Development Expert"),
        );
        assert_eq!(p, SubPattern::FilterInducedDrop);
    }

    #[test]
    fn each_variant_has_distinct_name_and_action() {
        let all = [
            SubPattern::UnregisteredClass,
            SubPattern::UnregisteredSubagent,
            SubPattern::FilterInducedDrop,
            SubPattern::MissingSystem,
            SubPattern::StringFormSystem,
            SubPattern::NonThreeBlockLayout,
            SubPattern::NonTextBlockAtIndex,
            SubPattern::Block2TextMissing,
            SubPattern::Unknown,
        ];
        let names: std::collections::HashSet<&'static str> = all.iter().map(|p| p.name()).collect();
        assert_eq!(
            names.len(),
            all.len(),
            "every SubPattern variant must produce a distinct name()"
        );
    }
}
