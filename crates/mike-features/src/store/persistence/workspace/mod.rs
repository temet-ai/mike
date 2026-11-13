//! Facade: `store::persistence::workspace` as mike_features sees it (crate split, ADR 0002).

use chrono::{DateTime, Utc};
use crate::workspace::Liveness;

pub use mike_base::store::persistence::workspace::*;
pub mod roster;
