//! A row of the /api/capabilities report, which feature health checks return.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum CapabilityState {
    Active,
    Degraded,
    Inert,
    Off,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CapabilityRow {
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    pub(crate) state: CapabilityState,
    pub(crate) detail: String,
    pub(crate) fix: String,
}
