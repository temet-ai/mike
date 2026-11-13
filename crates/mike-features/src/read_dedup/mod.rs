//! The read-dedup pipeline stage; the index and its logic are in wire::read_dedup.

pub mod stage;
#[cfg(test)]
pub mod stage_decline_tests;

pub use crate::wire::read_dedup::*;
