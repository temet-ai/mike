use bytes::Bytes;

use super::blocks::BlockTriple;
use super::decision::Composition;

#[derive(Debug, thiserror::Error)]
pub(crate) enum ComposeError {
    #[error("composition row has no text in block 2 (database constraint should prevent this)")]
    CompositionMissingBlock2Text,
    #[error(
        "could not locate the original block-2 text bytes in the raw body — \
         escape-sequence mismatch between the sender's JSON encoder and serde_json?"
    )]
    Block2TextNotFoundVerbatim,
    #[error(
        "the original block-2 text appears more than once in the raw body — \
         splice would be ambiguous"
    )]
    Block2TextAmbiguous,
    #[error("failed to JSON-encode block-2 text: {0}")]
    Encode(#[from] serde_json::Error),
}

pub(super) fn compose_template(
    raw_body: &[u8],
    triple: &BlockTriple,
    composition: &Composition,
) -> Result<Bytes, ComposeError> {
    let new_block2_text: &str = composition
        .blocks
        .get(2)
        .and_then(|b| b.get("text"))
        .and_then(|t| t.as_str())
        .ok_or(ComposeError::CompositionMissingBlock2Text)?;

    let original_encoded = serde_json::to_vec(&triple.block2_text)?;
    let new_encoded = serde_json::to_vec(new_block2_text)?;

    let needle = original_encoded.as_slice();
    let first_idx =
        find_subsequence(raw_body, needle).ok_or(ComposeError::Block2TextNotFoundVerbatim)?;

    let after_first = &raw_body[first_idx + needle.len()..];
    if find_subsequence(after_first, needle).is_some() {
        return Err(ComposeError::Block2TextAmbiguous);
    }

    let mut new_body = Vec::with_capacity(raw_body.len() - needle.len() + new_encoded.len());
    new_body.extend_from_slice(&raw_body[..first_idx]);
    new_body.extend_from_slice(&new_encoded);
    new_body.extend_from_slice(&raw_body[first_idx + needle.len()..]);

    Ok(Bytes::from(new_body))
}

pub(super) fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
