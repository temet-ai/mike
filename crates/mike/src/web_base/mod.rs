//! Request-handling helpers shared by every crate that serves inspect routes:
//! error responses, the change source of a request, search windows and result
//! ordering. Part of the base of the crate split (docs/adr/0002-split-crate-mike.md),
//! so nothing here may reach into a feature or into `api`.

pub(crate) mod body_search;
pub(crate) mod change_source;
pub(crate) mod detail;
mod errors;
pub(crate) mod result_order;
pub(crate) mod search_window;

pub(crate) use errors::{persist_error_response, persist_unavailable_response};
