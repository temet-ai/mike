//! Facade: `api::rerank_search` as mike sees it (crate split, ADR 0002).

pub use mike_features::api::rerank_search::*;
#[cfg(test)]
pub mod harness;
