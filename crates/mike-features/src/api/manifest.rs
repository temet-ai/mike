//! Facade: `api::manifest` as mike_features sees it (crate split, ADR 0002).

pub use crate::web_base::route_manifest::{Coverage, RouteEntry};

pub mod fleets;
