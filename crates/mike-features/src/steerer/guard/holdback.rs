use sha2::{Digest, Sha256};

use super::Question;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArmAnchor {
    pub steerer_id: i64,
    pub question: Question,
    pub anchor_turn_id: i64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Arm {
    Treated { draw: f64 },
    Holdback { draw: f64 },
}

impl Arm {}

pub fn holdback_draw(salt: uuid::Uuid, anchor: &ArmAnchor) -> f64 {
    let key = format!(
        "{salt}:{}:{}:{}",
        anchor.steerer_id,
        anchor.question.code(),
        anchor.anchor_turn_id
    );
    let digest = Sha256::digest(key.as_bytes());
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    u64::from_be_bytes(bytes) as f64 / 18_446_744_073_709_551_616.0
}

pub fn assign_arm(salt: uuid::Uuid, anchor: &ArmAnchor, pct: u8) -> Arm {
    let draw = holdback_draw(salt, anchor);
    if draw < f64::from(pct.min(50)) / 100.0 {
        Arm::Holdback { draw }
    } else {
        Arm::Treated { draw }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    fn anchor(turn: i64) -> ArmAnchor {
        ArmAnchor {
            steerer_id: 17,
            question: Question::Relapse,
            anchor_turn_id: turn,
        }
    }

    #[test]
    fn holdback_draw_is_reproducible_from_the_salt() {
        let salt = uuid::Uuid::from_u128(42);
        assert_eq!(
            holdback_draw(salt, &anchor(7)),
            holdback_draw(salt, &anchor(7))
        );
        assert_ne!(
            holdback_draw(salt, &anchor(7)),
            holdback_draw(salt, &anchor(8))
        );
        let d = holdback_draw(salt, &anchor(7));
        assert!((0.0..1.0).contains(&d));
    }

    #[test]
    fn assign_arm_ten_thousand_anchors_hold_pct_within_binomial_bound() {
        let salt = uuid::Uuid::from_u128(7);
        let held = (0..10_000)
            .filter(|t| matches!(assign_arm(salt, &anchor(*t), 10), Arm::Holdback { .. }))
            .count() as f64;
        let sd = (10_000.0_f64 * 0.1 * 0.9).sqrt();
        assert!((held - 1_000.0).abs() < 4.0 * sd, "held back {held}");
    }

    #[test]
    fn assign_arm_zero_pct_never_holds_back() {
        let salt = uuid::Uuid::from_u128(9);
        assert!((0..500).all(|t| matches!(assign_arm(salt, &anchor(t), 0), Arm::Treated { .. })));
    }
}
