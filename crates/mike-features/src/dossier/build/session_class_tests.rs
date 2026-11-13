use super::session_class::{LaneSummary, LaunchFacts, SessionClass, SessionClassInput, classify};

fn launch(seat: Option<&str>, run: Option<&str>, fork: bool) -> LaunchFacts {
    LaunchFacts {
        launch_id: 1,
        run_id: run.map(str::to_string),
        seat: seat.map(str::to_string),
        template_id: None,
        fork,
    }
}

fn base() -> SessionClassInput {
    SessionClassInput {
        lanes: LaneSummary {
            primary_without_agent_id: 5,
            agent_id_requests: 0,
            workflow_agent_requests: 0,
        },
        ..SessionClassInput::default()
    }
}

fn class_and_rule(input: &SessionClassInput) -> (SessionClass, &'static str) {
    let result = classify(input);
    (result.class, result.rule)
}

#[test]
fn template_session_is_template() {
    let input = SessionClassInput {
        is_template_session: true,
        ..base()
    };
    assert_eq!(class_and_rule(&input), (SessionClass::Template, "template"));
}

#[test]
fn launched_seat_is_fleet_seat_by_launch() {
    let input = SessionClassInput {
        launches: vec![launch(Some("jc21-b"), Some("run-1"), false)],
        ..base()
    };
    assert_eq!(class_and_rule(&input), (SessionClass::FleetSeat, "launch"));
}

#[test]
fn tmp_project_path_is_synthetic() {
    let input = SessionClassInput {
        project_path: Some("/tmp/probe-x".to_string()),
        ..base()
    };
    assert_eq!(class_and_rule(&input), (SessionClass::Synthetic, "path"));
    let scratch = SessionClassInput {
        project_path: Some("/home/u/x/scratchpad/y".to_string()),
        ..base()
    };
    assert_eq!(class_and_rule(&scratch).1, "path");
}

#[test]
fn probe_first_prompt_is_synthetic() {
    let input = SessionClassInput {
        first_prompt: Some("Reply with exactly the word OK".to_string()),
        ..base()
    };
    assert_eq!(class_and_rule(&input), (SessionClass::Synthetic, "probe"));
    let greeting = SessionClassInput {
        first_prompt: Some("Say hello".to_string()),
        ..base()
    };
    assert_eq!(class_and_rule(&greeting).1, "probe");
}

#[test]
fn only_workflow_agent_lanes_is_workflow() {
    let input = SessionClassInput {
        lanes: LaneSummary {
            primary_without_agent_id: 0,
            agent_id_requests: 4,
            workflow_agent_requests: 4,
        },
        ..SessionClassInput::default()
    };
    assert_eq!(class_and_rule(&input), (SessionClass::Workflow, "workflow"));
}

#[test]
fn only_agent_id_requests_is_subagent() {
    let input = SessionClassInput {
        lanes: LaneSummary {
            primary_without_agent_id: 0,
            agent_id_requests: 4,
            workflow_agent_requests: 1,
        },
        ..SessionClassInput::default()
    };
    assert_eq!(class_and_rule(&input), (SessionClass::Subagent, "subagent"));
}

#[test]
fn role_reviewer_without_a_fleet_launch_is_operator() {
    let input = SessionClassInput {
        role: Some("reviewer".to_string()),
        ..base()
    };
    assert_eq!(class_and_rule(&input), (SessionClass::Operator, "default"));
}

#[test]
fn role_coordinator_is_operator() {
    let input = SessionClassInput {
        role: Some("coordinator".to_string()),
        ..base()
    };
    assert_eq!(class_and_rule(&input), (SessionClass::Operator, "default"));
}

#[test]
fn precedence_template_over_launch() {
    let input = SessionClassInput {
        is_template_session: true,
        launches: vec![launch(Some("seat"), Some("run"), false)],
        ..base()
    };
    assert_eq!(classify(&input).class, SessionClass::Template);
}

#[test]
fn template_seat_name_is_template() {
    let input = SessionClassInput {
        launches: vec![launch(Some("template-rust"), Some("run"), false)],
        ..base()
    };
    assert_eq!(class_and_rule(&input), (SessionClass::Template, "template"));
}

#[test]
fn fork_without_run_is_operator() {
    let input = SessionClassInput {
        launches: vec![launch(None, None, true)],
        ..base()
    };
    assert_eq!(class_and_rule(&input), (SessionClass::Operator, "default"));
}

#[test]
fn coordinator_seat_is_operator() {
    let input = SessionClassInput {
        launches: vec![launch(Some("coordinator-1"), Some("run"), false)],
        ..base()
    };
    assert_eq!(class_and_rule(&input), (SessionClass::Operator, "default"));
}

#[test]
fn runless_seat_with_later_genuine_turn_is_operator() {
    let input = SessionClassInput {
        launches: vec![launch(Some("dawid"), None, false)],
        later_genuine_turn_id: Some(99),
        ..base()
    };
    assert_eq!(class_and_rule(&input), (SessionClass::Operator, "default"));
}

#[test]
fn runless_seat_with_only_brief_is_operator() {
    let input = SessionClassInput {
        launches: vec![launch(Some("dawid"), None, false)],
        ..base()
    };
    assert_eq!(class_and_rule(&input), (SessionClass::Operator, "default"));
}

#[test]
fn evidence_names_the_deciding_launch() {
    let input = SessionClassInput {
        launches: vec![launch(Some("jc21-b"), Some("run-1"), true)],
        role: Some("rust-developer".to_string()),
        ..base()
    };
    let evidence = classify(&input).evidence;
    assert_eq!(evidence.rule, "launch");
    assert_eq!(evidence.launch_id, Some(1));
    assert_eq!(evidence.seat.as_deref(), Some("jc21-b"));
    assert!(evidence.fork);
    assert_eq!(evidence.role.as_deref(), Some("rust-developer"));
}
