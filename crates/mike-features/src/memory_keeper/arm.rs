use crate::lesson_delivery::bucket;

pub const KEEPER_ARM_SALT: &str = "memory-keeper:";
pub const KEEPER_CONTROL_PERCENT: u16 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeeperArm {
    Keeper,
    Control,
}

impl KeeperArm {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Keeper => "keeper",
            Self::Control => "control",
        }
    }
}

pub fn arm_for(session: &str) -> KeeperArm {
    if bucket(KEEPER_ARM_SALT, session) < KEEPER_CONTROL_PERCENT {
        KeeperArm::Control
    } else {
        KeeperArm::Keeper
    }
}

#[cfg(test)]
#[path = "arm_tests.rs"]
pub mod arm_tests;
