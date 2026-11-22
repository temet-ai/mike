pub mod canonicalize;
pub mod chain_bridge;
pub mod delta;
pub mod delta_correction;
pub mod delta_same;
pub mod envelope_patch;
pub mod predecessor;
pub mod reads;
pub mod sent_capture;
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
    CanonicalMessage, canonicalise_body_messages, insert_request_messages, write_request_messages,
};
pub use canonicalize::{hex_lowercase, invariant};
pub use delta::{DeltaRow, SentPatch};
pub use delta::{compute_delta, insert_request_deltas, write_request_deltas};
#[cfg(any(test, feature = "test-seams"))]
pub use envelope_patch::compute_envelope_patch;
pub use envelope_patch::whole_body_envelope_patch;
pub use predecessor::{PredecessorDelta, PredecessorRule, resolve_predecessor_and_delta};
pub use reads::fetch_canonical_bridge;
pub use reads::{PersistedDeltaRow, fetch_request_deltas_by_id};
pub use sent_capture::{envelope_patch_for_capture, patches_reconstruct_sent};
pub use sent_patch::{SentPatchRow, apply_sent_patches, compute_sent_patches};
