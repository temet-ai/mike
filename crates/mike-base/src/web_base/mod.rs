//! Request-handling helpers shared by every crate that serves inspect routes:
//! error responses, the change source of a request, search windows and result
//! ordering. Part of the base of the crate split (docs/adr/0002-split-crate-mike.md),
//! so nothing here may reach into a feature or into `api`.

pub mod body_search;
pub mod capability;
pub mod change_source;
pub mod detail;
pub mod errors;
pub mod host_guard;
pub mod http;
pub mod result_order;
pub mod route_manifest;
pub mod search_window;

pub use errors::{persist_error_response, persist_unavailable_response};
