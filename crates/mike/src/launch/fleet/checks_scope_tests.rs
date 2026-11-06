use super::*;

fn task(brief: &str, files: usize) -> TaskDefinition {
    TaskDefinition {
        brief: brief.into(),
        files: (0..files).map(|n| format!("src/f{n}.rs")).collect(),
        ..TaskDefinition::default()
    }
}

#[test]
fn a_live_exercise_must_name_what_makes_it_appear() {
    assert!(live_without_seed(&task("Exercise the tracker chip live in radar.", 1)).is_some());
    assert!(
        live_without_seed(&task(
            "Exercise it live; seed a tracker with fixtures/tracker.json.",
            1
        ))
        .is_none()
    );
    assert!(live_without_seed(&task("Delivery is to be checked on the deliverable.", 1)).is_none());
}

#[test]
fn a_wide_removal_must_be_split_into_compiling_slices() {
    assert!(removal_too_wide(&task("Remove the Ratatui painters.", 21)).is_some());
    assert!(removal_too_wide(&task("Remove the Ratatui painters.", 20)).is_none());
    assert!(removal_too_wide(&task("Port the parts to neutral types.", 40)).is_none());
}
