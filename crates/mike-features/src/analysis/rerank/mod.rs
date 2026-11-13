//! Facade: `analysis::rerank` as mike_features sees it (crate split, ADR 0002).


pub use mike_base::analysis::rerank::*;
pub mod warmup;
#[cfg(test)]
pub mod warmup_tests;
