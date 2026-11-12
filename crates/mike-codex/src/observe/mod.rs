mod hook;
mod otel;
mod tool_kind;

pub use hook::{HookEvent, HookPayload, ThreadScope};
pub use otel::{
    ObservedModelCall, RequestAttempt, TelemetryFact, ToolResult, log_facts, resource_version,
};
pub use tool_kind::{COVERAGE_GAPS, CoverageGap, ToolKind, from_hook_name, from_telemetry};

#[cfg(test)]
mod parse_tests;
