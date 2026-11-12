//! Configuration of the RSS cap manager (`memory_cap::manager`).

use std::time::Duration;

use serde::{Deserialize, Serialize};

mod duration_serde {
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S>(d: &Duration, ser: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ser.serialize_str(&humantime::format_duration(*d).to_string())
    }

    pub(super) fn deserialize<'de, D>(de: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(de)?;
        humantime::parse_duration(&s).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapManagerConfig {
    #[serde(with = "duration_serde")]
    pub interval: Duration,
    pub rss_ceiling_bytes: u64,
    pub rss_high_watermark_pct: u8,
    pub rss_low_watermark_pct: u8,
}

impl Default for CapManagerConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(1),
            rss_ceiling_bytes: 0,
            rss_high_watermark_pct: 90,
            rss_low_watermark_pct: 75,
        }
    }
}

impl CapManagerConfig {
    pub(crate) fn high_watermark(&self) -> u64 {
        self.rss_ceiling_bytes
            .saturating_mul(self.rss_high_watermark_pct as u64)
            / 100
    }
    pub(crate) fn low_watermark(&self) -> u64 {
        self.rss_ceiling_bytes
            .saturating_mul(self.rss_low_watermark_pct as u64)
            / 100
    }
}
