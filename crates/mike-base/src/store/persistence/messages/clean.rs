use sha2::{Digest, Sha256};

use super::canonicalize::CanonicalMessage;
use crate::store::persistence::text_sanitize;

pub fn clean_canonical_message(message: CanonicalMessage) -> CanonicalMessage {
    if !text_sanitize::body_carries_nul(&message.body_canonical_bytes) {
        return message;
    }
    let Ok(mut body) = serde_json::from_slice::<serde_json::Value>(&message.body_canonical_bytes)
    else {
        return message;
    };
    if !text_sanitize::clean_json(&mut body) {
        return message;
    }
    let Ok(bytes) = serde_json::to_vec(&body) else {
        return message;
    };
    let Ok(byte_size) = i32::try_from(bytes.len()) else {
        return message;
    };
    CanonicalMessage {
        sha: hex::encode(Sha256::digest(&bytes)),
        body_canonical_bytes: bytes,
        byte_size,
        ..message
    }
}

pub fn canonical_clean_messages(
    body: &[u8],
) -> Result<Vec<CanonicalMessage>, crate::store::persistence::PersistError> {
    Ok(super::canonicalise_body_messages(body)?
        .into_iter()
        .map(clean_canonical_message)
        .collect())
}
