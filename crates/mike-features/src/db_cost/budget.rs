use serde::Serialize;

use super::snapshot::Counters;
use crate::config::db_cost::DbCostConfig;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Hot,
    Read,
    Periodic,
}

impl Tier {
    pub fn name(self) -> &'static str {
        match self {
            Tier::Hot => "hot",
            Tier::Read => "read",
            Tier::Periodic => "periodic",
        }
    }
}

pub fn tier_for(calls_per_sec: f64, config: &DbCostConfig) -> Tier {
    if calls_per_sec > config.hot_above_calls_per_sec {
        Tier::Hot
    } else if calls_per_sec < config.periodic_below_calls_per_sec {
        Tier::Periodic
    } else {
        Tier::Read
    }
}

pub fn budget_ms(tier: Tier, config: &DbCostConfig) -> f64 {
    match tier {
        Tier::Hot => config.hot_budget_ms,
        Tier::Read => config.read_budget_ms,
        Tier::Periodic => config.periodic_budget_ms,
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StatementCost {
    pub queryid: String,
    pub query: Option<String>,
    pub calls: i64,
    pub generic_plan_calls: i64,
    pub custom_plan_calls: i64,
    pub calls_per_sec: f64,
    pub mean_ms: f64,
    pub tier: Tier,
    pub budget_ms: f64,
    pub over_budget: bool,
    pub over_cost_alarm: bool,
    pub db_secs: f64,
    pub db_secs_per_hour: f64,
    pub shared_blks_hit: i64,
    pub shared_blks_read: i64,
    pub shared_blks_dirtied: i64,
    pub shared_blks_written: i64,
}

impl StatementCost {
    pub fn offends(&self) -> bool {
        self.over_budget || self.over_cost_alarm
    }

    pub fn queryid_number(&self) -> Option<i64> {
        self.queryid.parse().ok()
    }
}

pub fn statement_cost(
    queryid: i64,
    delta: Counters,
    window_secs: f64,
    config: &DbCostConfig,
) -> StatementCost {
    let window_secs = window_secs.max(1.0);
    let calls_per_sec = delta.calls as f64 / window_secs;
    let mean_ms = if delta.calls > 0 {
        delta.exec_ms / delta.calls as f64
    } else {
        0.0
    };
    let tier = tier_for(calls_per_sec, config);
    let budget = budget_ms(tier, config);
    let db_secs = delta.exec_ms / 1000.0;
    let db_secs_per_hour = db_secs * 3600.0 / window_secs;
    StatementCost {
        queryid: queryid.to_string(),
        query: None,
        calls: delta.calls,
        generic_plan_calls: delta.generic_plan_calls,
        custom_plan_calls: delta.custom_plan_calls,
        calls_per_sec,
        mean_ms,
        tier,
        budget_ms: budget,
        over_budget: mean_ms > budget && db_secs_per_hour >= config.budget_floor_db_secs_per_hour,
        over_cost_alarm: db_secs_per_hour > config.statement_alarm_db_secs_per_hour,
        db_secs,
        db_secs_per_hour,
        shared_blks_hit: delta.blks_hit,
        shared_blks_read: delta.blks_read,
        shared_blks_dirtied: delta.blks_dirtied,
        shared_blks_written: delta.blks_written,
    }
}

pub fn by_cost_desc(a: &StatementCost, b: &StatementCost) -> std::cmp::Ordering {
    b.db_secs_per_hour
        .total_cmp(&a.db_secs_per_hour)
        .then_with(|| a.queryid.cmp(&b.queryid))
}

#[cfg(test)]
#[path = "budget_tests.rs"]
pub mod budget_tests;
