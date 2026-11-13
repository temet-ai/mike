use serde::{Deserialize, Serialize};

const DEFAULT_MIN_BLOCK_BYTES: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct CompressConfig {
    pub enabled: bool,

    pub min_block_bytes: usize,
}

impl Default for CompressConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            min_block_bytes: DEFAULT_MIN_BLOCK_BYTES,
        }
    }
}

impl CompressConfig {
    pub fn for_measurement(&self) -> Self {
        Self {
            enabled: true,
            min_block_bytes: self.min_block_bytes,
        }
    }
}
