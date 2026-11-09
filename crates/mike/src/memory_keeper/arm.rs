use sha2::{Digest, Sha256};

use crate::lesson_delivery::CONTROL_PERCENT;

pub(crate) const KEEPER_ARM_SALT: &str = "memory-keeper";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeeperArm {
    Keeper,
    Control,
}

impl KeeperArm {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Keeper => "keeper",
            Self::Control => "control",
        }
    }
}

pub(crate) fn arm_for(session: &str) -> KeeperArm {
    let mut hasher = Sha256::new();
    hasher.update(KEEPER_ARM_SALT.as_bytes());
    hasher.update(b":");
    hasher.update(session.as_bytes());
    let digest: [u8; 32] = hasher.finalize().into();
    let bucket = u16::from_be_bytes([digest[0], digest[1]]) % 100;
    if bucket < CONTROL_PERCENT {
        KeeperArm::Control
    } else {
        KeeperArm::Keeper
    }
}

#[cfg(test)]
#[path = "arm_tests.rs"]
mod arm_tests;
