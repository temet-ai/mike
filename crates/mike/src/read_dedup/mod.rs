//! The read-dedup pipeline stage; the index and its logic are in wire::read_dedup.

pub(crate) mod stage;
#[cfg(test)]
mod stage_decline_tests;

pub(crate) use crate::wire::read_dedup::*;
