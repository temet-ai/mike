use super::*;

fn delta(calls: i64, exec_ms: f64) -> Counters {
    Counters {
        calls,
        exec_ms,
        ..Counters::default()
    }
}

#[test]
fn tier_for_more_than_one_call_per_second_is_hot() {
    let config = DbCostConfig::default();
    assert_eq!(tier_for(1.5, &config), Tier::Hot);
}

#[test]
fn tier_for_one_call_per_second_or_less_is_read() {
    let config = DbCostConfig::default();
    assert_eq!(tier_for(1.0, &config), Tier::Read);
    assert_eq!(tier_for(0.05, &config), Tier::Read);
}

#[test]
fn tier_for_less_than_one_call_a_minute_is_periodic() {
    let config = DbCostConfig::default();
    assert_eq!(tier_for(1.0 / 300.0, &config), Tier::Periodic);
}

#[test]
fn statement_cost_hot_poll_over_five_ms_is_over_budget() {
    let config = DbCostConfig::default();
    let cost = statement_cost(7, delta(7200, 7200.0 * 60.0), 3600.0, &config);
    assert_eq!(cost.tier, Tier::Hot);
    assert!((cost.mean_ms - 60.0).abs() < 1e-9);
    assert!((cost.budget_ms - 5.0).abs() < f64::EPSILON);
    assert!(cost.over_budget);
    assert!((cost.db_secs_per_hour - 432.0).abs() < 1e-9);
    assert!(cost.over_cost_alarm);
    assert!(cost.offends());
    assert_eq!(cost.queryid, "7");
}

#[test]
fn statement_cost_read_under_fifty_ms_is_within_budget() {
    let config = DbCostConfig::default();
    let cost = statement_cost(-3, delta(360, 360.0 * 40.0), 3600.0, &config);
    assert_eq!(cost.tier, Tier::Read);
    assert!(!cost.over_budget);
    assert!(!cost.offends());
    assert_eq!(cost.queryid_number(), Some(-3));
}

#[test]
fn statement_cost_slow_but_cheap_statement_stays_under_the_floor() {
    let config = DbCostConfig::default();
    let cost = statement_cost(1, delta(2, 2.0 * 900.0), 3600.0, &config);
    assert_eq!(cost.tier, Tier::Periodic);
    assert!(cost.mean_ms > cost.budget_ms);
    assert!(cost.db_secs_per_hour < config.budget_floor_db_secs_per_hour);
    assert!(!cost.over_budget);
}

#[test]
fn statement_cost_fast_statement_called_constantly_trips_the_cost_alarm() {
    let config = DbCostConfig::default();
    let cost = statement_cost(2, delta(400_000, 400_000.0 * 1.0), 3600.0, &config);
    assert_eq!(cost.tier, Tier::Hot);
    assert!(!cost.over_budget);
    assert!(cost.over_cost_alarm);
    assert!(cost.offends());
}

#[test]
fn by_cost_desc_orders_by_db_time_then_queryid() {
    let config = DbCostConfig::default();
    let cheap = statement_cost(1, delta(10, 10.0), 600.0, &config);
    let dear = statement_cost(2, delta(10, 9000.0), 600.0, &config);
    let tie = statement_cost(3, delta(10, 9000.0), 600.0, &config);
    let mut all = vec![cheap.clone(), tie.clone(), dear.clone()];
    all.sort_by(by_cost_desc);
    assert_eq!(all, vec![dear, tie, cheap]);
}
