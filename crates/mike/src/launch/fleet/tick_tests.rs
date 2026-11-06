use super::{STEPS, should_close};

#[test]
fn the_tick_runs_staff_before_it_tells_anyone_and_closes_last() {
    let names: Vec<&str> = STEPS.iter().map(|step| step.name).collect();
    assert_eq!(
        names,
        vec![
            "staff",
            "supervise",
            "settle_checks",
            "shared_tree",
            "judge",
            "problems",
            "introduce",
            "wake",
            "tell",
            "escalate",
            "close"
        ]
    );
}

#[test]
fn the_loop_closes_a_running_fleet_once_every_task_is_accepted_and_no_control_seat_is_live() {
    assert!(should_close("running", true, false));
    assert!(!should_close("running", false, false));
    assert!(
        !should_close("running", true, true),
        "a live coordinator or acceptance seat closes the fleet itself"
    );
    assert!(!should_close("stopping", true, false));
    assert!(!should_close("done", true, false));
}

#[test]
fn steps_include_problems_after_shared_tree() {
    let names: Vec<&str> = STEPS.iter().map(|step| step.name).collect();
    let shared = names.iter().position(|name| *name == "shared_tree");
    let problems = names.iter().position(|name| *name == "problems");
    assert!(shared.is_some() && problems.is_some() && shared < problems);
}

#[test]
fn every_step_has_a_distinct_name() {
    let mut names: Vec<&str> = STEPS.iter().map(|step| step.name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), STEPS.len());
}
