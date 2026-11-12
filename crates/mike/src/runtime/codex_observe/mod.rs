mod home;
mod identity;
pub(crate) mod ingest;
mod session;
pub(crate) use home::resolve_bound_session;

pub(crate) const SUPPORTED_CODEX_VERSIONS: [&str; 1] = ["0.167.1"];

#[cfg(test)]
#[path = "ingest_tests.rs"]
mod ingest_tests;
