use crate::store::persistence::PersistError;

use super::delta::SentPatch;
use super::envelope_patch::{
    compute_envelope_patch, locate_top_level_fields, whole_body_envelope_patch,
};
use super::sent_patch::{SentPatchRow, apply_sent_patches};

pub(crate) fn envelope_patch_for_capture(
    request_body: &[u8],
    sent_body: &[u8],
) -> Result<Option<SentPatch>, PersistError> {
    if request_body == sent_body {
        return Ok(None);
    }
    match compute_envelope_patch(request_body, sent_body) {
        Ok(Some(patch)) => Ok(Some(patch)),
        Ok(None) if differs_only_inside_messages(request_body, sent_body) => Ok(None),
        _ => whole_body_envelope_patch(request_body, sent_body),
    }
}

pub(crate) fn patches_reconstruct_sent(
    request_body: &[u8],
    sent_body: &[u8],
    patches: impl IntoIterator<Item = SentPatch>,
) -> bool {
    let rows = patches
        .into_iter()
        .map(|p| SentPatchRow {
            sent_patch_start: p.start,
            sent_patch_end: p.end,
            sent_patch_bytes: p.bytes,
        })
        .collect();
    apply_sent_patches(request_body, rows).is_ok_and(|rebuilt| rebuilt == sent_body)
}

fn differs_only_inside_messages(request_body: &[u8], sent_body: &[u8]) -> bool {
    let (Some(req_fields), Some(sent_fields)) = (
        locate_top_level_fields(request_body),
        locate_top_level_fields(sent_body),
    ) else {
        return false;
    };
    let (Some(r), Some(s)) = (req_fields.get("messages"), sent_fields.get("messages")) else {
        return false;
    };
    request_body[..r.start] == sent_body[..s.start] && request_body[r.end..] == sent_body[s.end..]
}
