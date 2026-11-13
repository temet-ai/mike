//! Facade: `workspace` as mike_features sees it (crate split, ADR 0002).

use std::str::FromStr;

pub use mike_base::workspace::*;
pub mod checkouts;
