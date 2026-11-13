pub mod canonicalize;
pub mod delta;
pub mod envelope_patch;
pub mod reads;
pub mod sent_capture;
pub mod sent_patch;

#[cfg(test)]
pub mod tests;

pub const RELIABLE_SIDECAR_ORIGINS: &[&str] = &[
    "title_sidecar",
    "advisor_sidecar",
    "quota_probe",
    "monitor_sidecar",
    "count_tokens_sidecar",
    "web_fetch_sidecar",
    "next_input_suggestion_sidecar",
];

pub use canonicalize::{CanonicalMessage, canonicalise_body_messages, write_request_messages};
pub use canonicalize::{hex_lowercase, invariant};
pub use delta::{DeltaRow, SentPatch};
pub use delta::{compute_delta, write_request_deltas};
#[cfg(test)]
pub use envelope_patch::compute_envelope_patch;
pub use envelope_patch::whole_body_envelope_patch;
pub use reads::{PersistedDeltaRow, fetch_request_deltas_by_id};
pub use reads::{PredecessorDelta, fetch_canonical_bridge, resolve_predecessor_and_delta};
pub use sent_capture::{envelope_patch_for_capture, patches_reconstruct_sent};
pub use sent_patch::{SentPatchRow, apply_sent_patches, compute_sent_patches};
