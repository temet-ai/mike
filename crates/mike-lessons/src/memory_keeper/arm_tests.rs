use super::*;

fn ids(count: usize) -> Vec<String> {
    (0..count).map(|n| format!("session-{n}")).collect()
}

fn keeper_control(id: &str) -> bool {
    arm_for(id) == KeeperArm::Control
}

#[test]
fn arm_for_same_session_twice_is_stable() {
    let id = "5d1c3b5e-0d3c-4a4e-9a55-3f2a0c0de001";
    assert_eq!(arm_for(id), arm_for(id));
}

#[test]
fn arm_for_many_sessions_puts_about_a_fifth_in_control() {
    let control = ids(2000).iter().filter(|id| keeper_control(id)).count();
    assert!(
        (300..=500).contains(&control),
        "control sessions: {control}"
    );
}
