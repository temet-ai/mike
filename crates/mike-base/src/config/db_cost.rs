use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DbCostConfig {
    pub interval_secs: u64,

    pub window_secs: u64,

    pub hot_budget_ms: f64,

    pub read_budget_ms: f64,

    pub periodic_budget_ms: f64,

    pub hot_above_calls_per_sec: f64,

    pub periodic_below_calls_per_sec: f64,

    pub budget_floor_db_secs_per_hour: f64,

    pub statement_alarm_db_secs_per_hour: f64,

    pub total_alarm_db_secs_per_hour: f64,

    pub top_statements: usize,
}

impl Default for DbCostConfig {
    fn default() -> Self {
        Self {
            interval_secs: 300,
            window_secs: 3600,
            hot_budget_ms: 5.0,
            read_budget_ms: 50.0,
            periodic_budget_ms: 500.0,
            hot_above_calls_per_sec: 1.0,
            periodic_below_calls_per_sec: 1.0 / 60.0,
            budget_floor_db_secs_per_hour: 10.0,
            statement_alarm_db_secs_per_hour: 300.0,
            total_alarm_db_secs_per_hour: 900.0,
            top_statements: 10,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_positive_and_tiers_are_ordered() {
        let cfg = DbCostConfig::default();
        assert!(cfg.interval_secs > 0, "a zero interval would busy-loop");
        assert!(cfg.window_secs >= cfg.interval_secs);
        assert!(cfg.hot_budget_ms < cfg.read_budget_ms);
        assert!(cfg.read_budget_ms < cfg.periodic_budget_ms);
        assert!(cfg.periodic_below_calls_per_sec < cfg.hot_above_calls_per_sec);
        assert!(cfg.statement_alarm_db_secs_per_hour <= cfg.total_alarm_db_secs_per_hour);
        assert!(cfg.top_statements > 0);
    }

    #[test]
    fn a_partial_toml_section_keeps_the_other_defaults() -> anyhow::Result<()> {
        let cfg: DbCostConfig = toml::from_str("hot_budget_ms = 2.5")?;
        assert!((cfg.hot_budget_ms - 2.5).abs() < f64::EPSILON);
        assert_eq!(cfg.window_secs, DbCostConfig::default().window_secs);
        Ok(())
    }

    #[test]
    fn an_unknown_key_is_rejected() {
        let parsed: Result<DbCostConfig, _> = toml::from_str("hot_budget = 2.5");
        assert!(parsed.is_err());
    }
}
