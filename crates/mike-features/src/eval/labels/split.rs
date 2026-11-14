pub use crate::features_settings::meta_eval::DEFAULT_HOLDOUT_PERCENT;

pub const SPLIT_SQL: &str = "CASE WHEN (((hashtextextended($1::text, 0) % 100) + 100) % 100) < $2 THEN 'holdout' ELSE 'dev' END";

pub fn clamp_percent(value: i64) -> i64 {
    value.clamp(0, 100)
}
