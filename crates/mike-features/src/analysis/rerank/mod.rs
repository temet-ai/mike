//! Facade: `analysis::rerank` as mike_features sees it (crate split, ADR 0002).

use serde::{Deserialize, Serialize};
use thiserror::Error;
use crate::config::deserialize_permissive_bool;

pub use mike_base::analysis::rerank::*;
pub mod warmup;
#[cfg(test)]
pub mod warmup_tests;
