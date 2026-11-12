mod apply;
mod hook;
pub(crate) mod ingest;
mod otel;
pub(crate) mod reads;
pub(crate) mod tool_kind;
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
