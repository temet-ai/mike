use std::collections::HashSet;

use super::*;

const ALL_KINDS: [Profile; 8] = [
    Profile::ProblemSolver,
    Profile::Worker,
    Profile::Coordinator,
    Profile::Acceptance,
    Profile::Observer,
    Profile::Retrospective,
    Profile::Reference,
    Profile::Core,
];

fn names(profile: Profile) -> HashSet<String> {
    tools_for(profile)
        .iter()
        .map(|t| t.name.as_ref().to_string())
        .collect()
}

#[test]
fn profile_names_parse_whichever_the_case() {
    for (text, profile) in [
        ("worker", Profile::Worker),
        ("Coordinator", Profile::Coordinator),
        (" ACCEPTANCE ", Profile::Acceptance),
        ("observer", Profile::Observer),
        (" RETROSPECTIVE ", Profile::Retrospective),
        ("reference", Profile::Reference),
        ("core", Profile::Core),
        ("lean", Profile::Core),
        ("full", Profile::Full),
    ] {
        assert_eq!(Profile::parse(text), Some(profile), "{text}");
    }
    assert_eq!(Profile::parse("seat"), None);
    assert_eq!(Profile::parse(""), None);
}

#[test]
fn the_default_profile_is_core() {
    assert_eq!(Profile::default(), Profile::Core);
}

#[test]
fn retrospective_lists_only_inspection_and_its_two_bounded_writes() {
    let expected = RETROSPECTIVE_TOOLS
        .iter()
        .map(|name| name.to_string())
        .collect();
    assert_eq!(names(Profile::Retrospective), expected);
    assert_eq!(RETROSPECTIVE_TOOLS.len(), 8);
    for name in [
        "write_session_note",
        "end_workspace_registration",
        "get_request_body",
    ] {
        assert!(names(Profile::Retrospective).contains(name));
    }
}

#[test]
fn every_listed_name_is_a_real_tool_and_listed_once() {
    let real: HashSet<String> = all_tools()
        .iter()
        .map(|t| t.name.as_ref().to_string())
        .collect();
    for profile in ALL_KINDS {
        let listed = served_names(profile);
        let unique: HashSet<&&str> = listed.iter().collect();
        assert_eq!(unique.len(), listed.len(), "{profile:?} lists a name twice");
        for name in listed {
            assert!(
                real.contains(name),
                "{profile:?} lists {name:?}, which is not a tool in all_tools()"
            );
        }
    }
}

#[test]
fn a_profile_serves_exactly_the_tools_it_lists() {
    for profile in ALL_KINDS {
        let served = names(profile);
        let listed: HashSet<String> = served_names(profile)
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(served, listed, "{profile:?}");
    }
}

#[test]
fn problem_solver_decides_and_reads_but_cannot_edit_report_or_start_anything() {
    let served = names(Profile::ProblemSolver);
    assert!(served.contains("fleet_decide"));
    let allowed: HashSet<&str> = PROBLEM_SOLVER_TOOLS.iter().copied().collect();
    assert_eq!(served.len(), allowed.len());
    for write in [
        "fleet_commit",
        "fleet_check",
        "fleet_report",
        "fleet_verdict",
        "fleet_propose",
        "fleet_split",
        "define_fleet",
        "spawn_session",
        "kill_session",
        "record_lesson",
        "create_filter_rule",
        "update_filter_rule",
        "delete_filter_rule",
    ] {
        assert!(!served.contains(write), "problem_solver serves {write}");
    }
}

#[test]
fn only_problem_solver_serves_fleet_decide() {
    for profile in ALL_KINDS {
        assert_eq!(
            names(profile).contains("fleet_decide"),
            profile == Profile::ProblemSolver,
            "{profile:?}"
        );
    }
    assert!(names(Profile::Full).contains("fleet_decide"));
}

#[test]
fn problem_solver_parses_by_either_spelling() {
    assert_eq!(
        Profile::parse("problem_solver"),
        Some(Profile::ProblemSolver)
    );
    assert_eq!(
        Profile::parse(" Problem-Solver "),
        Some(Profile::ProblemSolver)
    );
}

#[test]
fn full_serves_every_tool() {
    assert_eq!(tools_for(Profile::Full).len(), all_tools().len());
}

#[test]
fn only_full_serves_the_five_tools_a_launched_seat_does_not_need() {
    assert_eq!(FULL_ONLY_TOOLS.len(), 5);
    let full = names(Profile::Full);
    for tool in FULL_ONLY_TOOLS {
        assert!(full.contains(*tool), "full lacks {tool}");
        for profile in ALL_KINDS {
            assert!(
                !names(profile).contains(*tool),
                "{profile:?} serves {tool}, which only full may"
            );
        }
    }
}

#[test]
fn the_worker_profile_carries_what_a_fleet_worker_works_with() {
    let worker = names(Profile::Worker);
    for tool in [
        "get_fleet",
        "fleet_commit",
        "fleet_check",
        "fleet_where",
        "list_fleet_results",
        "list_fleet_changes",
        "what_changed",
        "send_session_message",
        "get_session_messages",
        "write_session_note",
        "list_session_notes",
        "find_text",
        "get_request_tool_calls",
        "list_session_tool_uses",
        "list_tool_errors",
    ] {
        assert!(worker.contains(tool), "worker lacks {tool}");
    }
    for tool in ["define_fleet", "spawn_session", "kill_session"] {
        assert!(!worker.contains(tool), "worker serves {tool}");
    }
}

#[test]
fn every_seat_reports_but_only_acceptance_gives_a_verdict() {
    for profile in [
        Profile::Worker,
        Profile::Coordinator,
        Profile::Acceptance,
        Profile::Observer,
        Profile::Reference,
    ] {
        let served = names(profile);
        assert!(served.contains("fleet_report"), "{profile:?}");
        assert_eq!(
            served.contains("fleet_verdict"),
            profile == Profile::Acceptance,
            "{profile:?}"
        );
        for tool in ["ask", "answer", "decline", "read_thread", "resolve_thread"] {
            assert!(served.contains(tool), "{profile:?} lacks {tool}");
        }
    }
}

#[test]
fn the_coordinator_can_define_and_stop_where_a_worker_cannot() {
    let coordinator = names(Profile::Coordinator);
    for tool in ["define_fleet", "spawn_session", "kill_session", "get_fleet"] {
        assert!(coordinator.contains(tool), "coordinator lacks {tool}");
    }
}

#[test]
fn acceptance_and_the_observer_read_requests_and_the_reference_seat_reads_without_writing() {
    for profile in [Profile::Acceptance, Profile::Observer, Profile::Reference] {
        let served = names(profile);
        for tool in ["get_request", "get_request_body", "get_session"] {
            assert!(served.contains(tool), "{profile:?} lacks {tool}");
        }
    }
    let reference = names(Profile::Reference);
    for tool in [
        "define_fleet",
        "fleet_commit",
        "spawn_session",
        "record_lesson",
    ] {
        let expected = tool == "fleet_commit" || tool == "record_lesson";
        assert_eq!(reference.contains(tool), expected, "reference {tool}");
    }
    assert!(names(Profile::Observer).contains("diagnostic_stats"));
}

#[test]
fn a_seat_profile_is_smaller_than_the_core_profile_it_replaces() {
    let core = names(Profile::Core).len();
    assert!(names(Profile::Worker).len() < core);
    assert!(names(Profile::Worker).len() <= names(Profile::Acceptance).len());
}

#[test]
fn every_tool_is_placed_in_a_profile_or_named_as_full_only() {
    let placed = placed_names();
    let missing: Vec<String> = all_tools()
        .iter()
        .map(|t| t.name.as_ref().to_string())
        .filter(|n| !placed.contains(n.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "tools in no profile: {missing:?}. Add each to a list in crates/mike-mcp/src/profiles.rs, or to ONLY_IN_FULL"
    );
}
