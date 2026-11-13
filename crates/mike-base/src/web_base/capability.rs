use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CapabilityState {
    Active,
    Degraded,
    Inert,
    Off,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapabilityRow {
    pub id: &'static str,
    pub label: &'static str,
    pub state: CapabilityState,
    pub detail: String,
    pub fix: String,
}
