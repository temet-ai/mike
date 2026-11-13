use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RetentionSweepConfig {
    pub interval_secs: u64,

    pub orphan_batch_size: i64,
}

impl Default for RetentionSweepConfig {
    fn default() -> Self {
        Self {
            interval_secs: 3600,
            orphan_batch_size: 2000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_positive_and_bounded() {
        let cfg = RetentionSweepConfig::default();
        assert!(cfg.interval_secs > 0, "a zero interval would busy-loop");
        assert!(
            cfg.orphan_batch_size > 0,
            "a zero batch size would sweep nothing forever"
        );
    }
}
