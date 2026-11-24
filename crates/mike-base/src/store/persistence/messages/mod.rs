pub mod canonicalize;
pub mod chain_bridge;
pub mod clean;
pub mod delta;
pub mod delta_correction;
pub mod delta_same;
pub mod predecessor;
pub mod reads;
pub mod sent_patch;

#[cfg(test)]
mod tests;

pub const RELIABLE_SIDECAR_ORIGINS: &[&str] = &[
    "title_sidecar",
    "advisor_sidecar",
    "quota_probe",
    "monitor_sidecar",
    "count_tokens_sidecar",
    "web_fetch_sidecar",
    "next_input_suggestion_sidecar",
    "agent_label_sidecar",
];

pub use canonicalize::{
    CanonicalMessage, canonicalise_body_messages, insert_new_message_bodies,
    insert_request_messages, write_request_messages,
};
pub use canonicalize::{hex_lowercase, invariant};
pub use clean::{canonical_clean_messages, clean_canonical_message};
pub use delta::DeltaRow;
pub use delta::{compute_delta, insert_request_deltas, write_request_deltas};
pub use predecessor::{PredecessorDelta, PredecessorRule, resolve_predecessor_and_delta};
pub use reads::fetch_canonical_bridge;
pub use reads::{PersistedDeltaRow, fetch_request_deltas_by_id};
pub use sent_patch::{SentPatchRow, apply_sent_patches};
