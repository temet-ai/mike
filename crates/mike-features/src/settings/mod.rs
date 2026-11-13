//! Facade: `settings` as mike_features sees it (crate split, ADR 0002).

use serde::{Deserialize, Serialize};
use crate::config::weasel_intercept::DEFAULT_CORRECTION_PROMPT;

pub use mike_base::settings::*;
pub mod service;
pub mod live_bundle_follow;
pub mod running_services;
#[cfg(test)]
pub mod running_services_tests;

#[cfg(test)]
pub mod settings_tests;
