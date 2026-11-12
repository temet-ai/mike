//! Configuration of the per-request compression stage (`compress`).

use serde::{Deserialize, Serialize};

const DEFAULT_MIN_BLOCK_BYTES: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct CompressConfig {
    pub(crate) enabled: bool,

    pub(crate) min_block_bytes: usize,
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
    pub(crate) fn for_measurement(&self) -> Self {
        Self {
            enabled: true,
            min_block_bytes: self.min_block_bytes,
        }
    }
}
