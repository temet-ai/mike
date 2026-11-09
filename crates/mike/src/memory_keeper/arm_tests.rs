use super::*;
use crate::lesson_delivery::{self, Arm};

fn ids(count: usize) -> Vec<String> {
    (0..count).map(|n| format!("session-{n}")).collect()
}

#[test]
fn arm_is_stable_for_a_session() {
    let id = "5d1c3b5e-0d3c-4a4e-9a55-3f2a0c0de001";
    assert_eq!(arm_for(id), arm_for(id));
}

#[test]
fn about_a_fifth_of_sessions_are_control() {
    let control = ids(2000)
        .iter()
        .filter(|id| arm_for(id) == KeeperArm::Control)
        .count();
    assert!(
        (300..=500).contains(&control),
        "control sessions: {control}"
    );
}

#[test]
fn both_arms_occur_in_a_fixed_set_of_ids() {
    let sample = ids(50);
    assert!(sample.iter().any(|id| arm_for(id) == KeeperArm::Keeper));
    assert!(sample.iter().any(|id| arm_for(id) == KeeperArm::Control));
}

#[test]
fn assignment_differs_from_the_lesson_arm_for_some_sessions() {
    let sample = ids(500);
    let differing = sample
        .iter()
        .filter(|id| {
            (arm_for(id) == KeeperArm::Control) != (lesson_delivery::arm_for(id) == Arm::Control)
        })
        .count();
    assert!(differing > 0);
}

#[test]
fn the_two_control_groups_overlap_only_by_chance() {
    let sample = ids(2000);
    let both = sample
        .iter()
        .filter(|id| {
            arm_for(id) == KeeperArm::Control && lesson_delivery::arm_for(id) == Arm::Control
        })
        .count();
    let keeper = sample
        .iter()
        .filter(|id| arm_for(id) == KeeperArm::Control)
        .count();
    assert!(both * 2 < keeper, "both: {both}, keeper control: {keeper}");
}

#[test]
fn arm_names_are_the_stored_values() {
    assert_eq!(KeeperArm::Keeper.as_str(), "keeper");
    assert_eq!(KeeperArm::Control.as_str(), "control");
}
