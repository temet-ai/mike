use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::questions::POINT;

pub const HOLDBACK_SHARE: f64 = 0.10;

pub fn draw(evaluation: Uuid) -> f64 {
    let digest = Sha256::digest(format!("{POINT}:holdback:{evaluation}").as_bytes());
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    u64::from_be_bytes(bytes) as f64 / 18_446_744_073_709_551_616.0
}

pub fn sent_anyway(evaluation: Option<Uuid>) -> bool {
    evaluation.is_some_and(|uuid| draw(uuid) < HOLDBACK_SHARE)
}
