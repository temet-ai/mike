use crate::steerer::registry::SteererRegistry;
use crate::store::persistence::operator_turns::QuiescenceTarget;

pub fn quiescence_targets(
    registry: &SteererRegistry,
    default_idle_secs: i64,
) -> Vec<QuiescenceTarget> {
    registry
        .enabled()
        .filter_map(|s| {
            let after = s.params.delivery.quiescence_after_secs?;
            Some(QuiescenceTarget {
                steerer_id: s.id,
                idle_after_secs: if after > 0 { after } else { default_idle_secs },
                question: s.params.delivery.quiescence_question.clone(),
            })
        })
        .collect()
}

#[cfg(test)]
pub mod tests {
    use super::quiescence_targets;
    use crate::steerer::registry::SteererRegistry;
    use crate::store::persistence::steering::SteererRow;

    fn row(id: i64, enabled: bool, config: serde_json::Value) -> SteererRow {
        SteererRow {
            id,
            uuid: uuid::Uuid::new_v4(),
            name: format!("steerer-{id}"),
            display_name: format!("Steerer {id}"),
            description: String::new(),
            agent_id: id,
            authority: "command".to_string(),
            mode: "drive".to_string(),
            model: String::new(),
            priority: 0,
            enabled,
            config,
            prompt: None,
            kind: "refocus".to_string(),
            trigger_point: "response".to_string(),
            invocation: "every_turn".to_string(),
            pattern: None,
            auto_enrol: false,
            project_id: None,
            project_path: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn targets_empty_when_no_steerer_opts_in() {
        let registry = SteererRegistry::from_rows(vec![row(1, true, serde_json::json!({}))]);
        assert!(quiescence_targets(&registry, 600).is_empty());
    }

    #[test]
    fn targets_skip_disabled_steerer() {
        let registry = SteererRegistry::from_rows(vec![row(
            1,
            false,
            serde_json::json!({"quiescence_after_secs": 90}),
        )]);
        assert!(quiescence_targets(&registry, 600).is_empty());
    }

    #[test]
    fn targets_carry_question_from_params() {
        let registry = SteererRegistry::from_rows(vec![row(
            7,
            true,
            serde_json::json!({"quiescence_after_secs": 90, "quiescence_question": "unbacked_wait"}),
        )]);
        let targets = quiescence_targets(&registry, 600);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].steerer_id, 7);
        assert_eq!(targets[0].idle_after_secs, 90);
        assert_eq!(targets[0].question.as_deref(), Some("unbacked_wait"));
    }
}
