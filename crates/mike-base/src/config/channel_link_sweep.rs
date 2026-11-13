use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ChannelLinkSweepConfig {
    pub interval_secs: u64,

    pub batch_size: i64,
}

impl Default for ChannelLinkSweepConfig {
    fn default() -> Self {
        Self {
            interval_secs: 60,
            batch_size: 200,
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn defaults_are_positive_and_bounded() {
        let cfg = ChannelLinkSweepConfig::default();
        assert!(cfg.interval_secs > 0, "a zero interval would busy-loop");
        assert!(
            cfg.batch_size > 0,
            "a zero batch size would sweep nothing forever"
        );
    }
}
