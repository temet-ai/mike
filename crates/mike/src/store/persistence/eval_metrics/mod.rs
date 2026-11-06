mod adhoc;
pub mod alarm_judge_call_loop;
mod alarm_predicates;
mod alarm_promotion;
mod alarms;
mod backtest;
mod band;
mod new_rates;
mod reads;
mod rollup;
mod shift;
mod sql_accuracy;
mod sql_base_rate;
mod sql_guard_calibration;
pub mod sql_label_mix;
mod sql_steer_calibration;
pub mod types;

pub use alarm_predicates::{RefusalRow, SubjectDetail, budget_setting_keys};
pub use rollup::{FAMILIES, Family, RollupCtx, RollupReport, RollupRequest};
pub use shift::{ShiftStats, shift_stats, tvd_and_chi2};
pub use types::{
    AckOutcome, AlarmKind, AlarmState, AlarmThresholds, AlarmsFilter, BacktestRow, EpisodeChange,
    EvalAlarmRow, EvalRateRow, EvalRollupRow, Firing, RatesFilter,
};

#[cfg(test)]
mod tests {
    const SOURCES: [&str; 8] = [
        include_str!("sql_accuracy.rs"),
        include_str!("sql_guard_calibration.rs"),
        include_str!("sql_label_mix.rs"),
        include_str!("sql_base_rate.rs"),
        include_str!("sql_steer_calibration.rs"),
        include_str!("alarm_predicates.rs"),
        include_str!("alarms.rs"),
        include_str!("alarm_promotion.rs"),
    ];

    #[test]
    fn eval_metrics_makes_no_llm_call() {
        let client = ["OpenRouter", "Client"].concat();
        let role = ["Llm", "Role"].concat();
        for source in SOURCES {
            assert!(!source.contains(&client));
            assert!(!source.contains(&role));
        }
    }

    #[test]
    fn alarm_kinds_round_trip_through_their_codes() {
        use super::AlarmKind;
        for kind in AlarmKind::OPERATIONAL.iter().chain(AlarmKind::DAILY.iter()) {
            assert_eq!(AlarmKind::parse(kind.code()), Some(*kind));
        }
        assert_eq!(AlarmKind::parse("metric_drop"), Some(AlarmKind::MetricDrop));
        assert_eq!(
            AlarmKind::parse("shadow_disagreement"),
            Some(AlarmKind::ShadowDisagreement)
        );
        assert_eq!(AlarmKind::parse("not_a_real_kind"), None);
    }
}
