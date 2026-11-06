use super::rollout_params::RolloutParams;
use crate::store::persistence::steering::SteererRow;

#[derive(Debug, Clone)]
pub(crate) struct SteererParams {
    pub(crate) confidence_threshold: f32,
    pub(crate) debounce_turns: u32,
    pub(crate) arming_window_turns: u32,
    pub(crate) suppress_first_n_turns: u32,
    pub(crate) delivery: RolloutParams,
}

impl Default for SteererParams {
    fn default() -> Self {
        Self {
            confidence_threshold: 0.7,
            debounce_turns: 3,
            arming_window_turns: 3,
            suppress_first_n_turns: 3,
            delivery: RolloutParams::default(),
        }
    }
}

impl SteererParams {
    fn from_json(v: &serde_json::Value) -> Self {
        let mut p = Self::default();
        if let Some(ct) = v.get("confidence_threshold").and_then(|v| v.as_f64()) {
            p.confidence_threshold = ct as f32;
        }
        if let Some(dt) = v.get("debounce_turns").and_then(|v| v.as_u64()) {
            p.debounce_turns = dt as u32;
        }
        if let Some(aw) = v.get("arming_window_turns").and_then(|v| v.as_u64()) {
            p.arming_window_turns = aw as u32;
        }
        if let Some(sf) = v.get("suppress_first_n_turns").and_then(|v| v.as_u64()) {
            p.suppress_first_n_turns = sf as u32;
        }
        p.delivery = RolloutParams::from_json(v);
        p
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Steerer {
    pub(crate) id: i64,
    pub(crate) uuid: uuid::Uuid,
    pub(crate) name: String,
    pub(crate) display_name: String,
    pub(crate) description: String,
    pub(crate) agent_id: i64,
    pub(crate) authority: String,
    pub(crate) mode: String,
    pub(crate) model: String,
    pub(crate) priority: i32,
    pub(crate) enabled: bool,
    pub(crate) params: SteererParams,
    pub(crate) config: serde_json::Value,
    pub(crate) prompt: Option<String>,
    pub(crate) kind: String,
    pub(crate) trigger_point: String,
    pub(crate) invocation: String,
    pub(crate) pattern: Option<String>,
    pub(crate) auto_enrol: bool,
    pub(crate) project_id: Option<i64>,
    pub(crate) project_path: Option<String>,
    pub(crate) created_at: chrono::DateTime<chrono::Utc>,
    pub(crate) updated_at: chrono::DateTime<chrono::Utc>,
}

impl TryFrom<&SteererRow> for Steerer {
    type Error = String;

    fn try_from(row: &SteererRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            uuid: row.uuid,
            name: row.name.clone(),
            display_name: row.display_name.clone(),
            description: row.description.clone(),
            agent_id: row.agent_id,
            authority: row.authority.clone(),
            mode: row.mode.clone(),
            model: row.model.clone(),
            priority: row.priority,
            enabled: row.enabled,
            params: SteererParams::from_json(&row.config),
            config: row.config.clone(),
            prompt: row.prompt.clone(),
            kind: row.kind.clone(),
            trigger_point: row.trigger_point.clone(),
            invocation: row.invocation.clone(),
            pattern: row.pattern.clone(),
            auto_enrol: row.auto_enrol,
            project_id: row.project_id,
            project_path: row.project_path.clone(),
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SteererRegistry {
    pub(crate) steerers: Vec<Steerer>,
}

impl SteererRegistry {
    pub(crate) fn from_rows(rows: Vec<SteererRow>) -> Self {
        let steerers = rows
            .iter()
            .filter_map(|row| match Steerer::try_from(row) {
                Ok(s) => Some(s),
                Err(e) => {
                    tracing::warn!(
                        steerer_id = row.id,
                        name = %row.name,
                        error = %e,
                        "steerer registry: failed to convert row — skipping"
                    );
                    None
                }
            })
            .collect();
        Self { steerers }
    }

    pub(crate) fn enabled(&self) -> impl Iterator<Item = &Steerer> {
        self.steerers.iter().filter(|s| s.enabled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_row(id: i64, enabled: bool, config: serde_json::Value) -> SteererRow {
        SteererRow {
            id,
            uuid: uuid::Uuid::new_v4(),
            name: format!("steerer-{id}"),
            display_name: format!("Steerer {id}"),
            description: String::new(),
            agent_id: id,
            authority: "redirect".to_string(),
            mode: "augment".to_string(),
            model: "google/gemini-3.5-flash-lite".to_string(),
            priority: id as i32,
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
    fn steerer_params_defaults_when_empty_config() -> anyhow::Result<()> {
        let row = make_row(1, true, serde_json::json!({}));
        let steerer = Steerer::try_from(&row).map_err(anyhow::Error::msg)?;
        assert!((steerer.params.confidence_threshold - 0.7).abs() < f32::EPSILON);
        assert_eq!(steerer.params.debounce_turns, 3);
        assert_eq!(steerer.params.arming_window_turns, 3);
        assert_eq!(steerer.params.suppress_first_n_turns, 3);
        Ok(())
    }

    #[test]
    fn steerer_params_parsed_from_config() -> anyhow::Result<()> {
        let row = make_row(
            1,
            true,
            serde_json::json!({
                "confidence_threshold": 0.85,
                "debounce_turns": 5,
                "arming_window_turns": 2,
                "suppress_first_n_turns": 4
            }),
        );
        let steerer = Steerer::try_from(&row).map_err(anyhow::Error::msg)?;
        assert!((steerer.params.confidence_threshold - 0.85).abs() < f32::EPSILON);
        assert_eq!(steerer.params.debounce_turns, 5);
        assert_eq!(steerer.params.arming_window_turns, 2);
        assert_eq!(steerer.params.suppress_first_n_turns, 4);
        Ok(())
    }

    #[test]
    fn steerer_params_ignores_unknown_keys() -> anyhow::Result<()> {
        let row = make_row(
            1,
            true,
            serde_json::json!({
                "confidence_threshold": 0.6,
                "future_field": "ignored"
            }),
        );
        let steerer = Steerer::try_from(&row).map_err(anyhow::Error::msg)?;
        assert!((steerer.params.confidence_threshold - 0.6).abs() < f32::EPSILON);
        assert_eq!(steerer.params.debounce_turns, 3);
        assert_eq!(steerer.params.arming_window_turns, 3);
        assert_eq!(steerer.params.suppress_first_n_turns, 3);
        Ok(())
    }

    #[test]
    fn steerer_keeps_the_stored_config_verbatim() -> anyhow::Result<()> {
        let stored = serde_json::json!({
            "debounce_turns": 2,
            "class": "efficiency",
            "requires": [{"binary": "ast-grep"}]
        });
        let row = make_row(1, true, stored.clone());
        let steerer = Steerer::try_from(&row).map_err(anyhow::Error::msg)?;
        assert_eq!(steerer.config, stored);
        Ok(())
    }

    #[test]
    fn registry_from_rows_filters_enabled() {
        let rows = vec![
            make_row(1, true, serde_json::json!({})),
            make_row(2, false, serde_json::json!({})),
            make_row(3, true, serde_json::json!({})),
        ];
        let registry = SteererRegistry::from_rows(rows);
        assert_eq!(registry.steerers.len(), 3);
        let enabled: Vec<_> = registry.enabled().collect();
        assert_eq!(enabled.len(), 2);
        assert!(enabled.iter().all(|s| s.enabled));
    }

    #[test]
    fn registry_empty_default() {
        let registry = SteererRegistry::default();
        assert_eq!(registry.steerers.len(), 0);
        assert_eq!(registry.enabled().count(), 0);
    }
}
