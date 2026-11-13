//! Facade: `api::manifest` as mike_features sees it (crate split, ADR 0002).

use std::time::Duration;
use axum::Router;
use axum::http::{HeaderName, Method, StatusCode, header};
use axum::middleware::from_fn_with_state;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::request_id::{PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use crate::state::AppState;
use crate::web_base::host_guard;
pub use crate::web_base::route_manifest::{Coverage, RouteEntry};

pub mod fleets;
