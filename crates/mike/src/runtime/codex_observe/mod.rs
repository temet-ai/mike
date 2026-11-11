pub(crate) mod api;
mod apply;
mod hook;
mod ingest;
mod otel;
mod reads;
mod tool_kind;
mod writes;

pub(crate) const ADAPTER: &str = "mike-codex-observe";
pub(crate) const ADAPTER_VERSION: &str = "1";
pub(crate) const SUPPORTED_CODEX_VERSIONS: [&str; 1] = ["0.167.1"];

#[cfg(test)]
#[path = "parse_tests.rs"]
mod parse_tests;

#[cfg(test)]
#[path = "ingest_tests.rs"]
mod ingest_tests;
