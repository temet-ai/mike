//! Facade: `store` as mike_features sees it (crate split, ADR 0002).

#[cfg(any(test, feature = "test-seams"))]
pub use persistence::test_support;
use arc_swap::ArcSwap;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::broadcast;
use crate::analysis::{AnalyzerRegistry, EmbeddingService};
use persistence::PersistHandle;

pub use mike_base::store::*;
pub mod persistence;
