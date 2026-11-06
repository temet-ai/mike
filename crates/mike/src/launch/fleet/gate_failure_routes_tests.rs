use super::*;

fn fixture() -> (FleetDefinition, Target, Vec<String>) {
    let mut def = FleetDefinition {
        repo: "/repo".into(),
        ..FleetDefinition::default()
    };
    for (id, path, session) in [("A", "ink/src", 1), ("B", "ink/tests", 2)] {
        def.tasks.insert(
            id.into(),
            TaskDefinition {
                files: vec![path.into()],
                seat: Some(format!("worker-{id}")),
                session: Some(Uuid::from_u128(session).to_string()),
                ..TaskDefinition::default()
            },
        );
    }
    let target = Target {
        commit: "abcdef".into(),
        pushed: true,
        slot: "/repo/.claude/worktrees/gate".into(),
    };
    let files = ["ink/src/a.test.ts", "ink/tests/b.test.ts", "other.test.ts"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    (def, target, files)
}

#[test]
fn distinct_failure_files_route_each_owner_once_even_after_timeout() {
    let (def, target, files) = fixture();
    let output = "FAIL src/a.test.ts > first\nFAIL tests/b.test.ts > second\nFAIL src/a.test.ts > first\nthe gate was stopped after 120 seconds";
    let routed = routes(&def, &target, &files, output);
    assert_eq!(routed.owners.len(), 2);
    assert!(routed.unresolved.is_empty());
    for (session, expected) in [(1, "first"), (2, "second")] {
        let details = &routed.owners[&Uuid::from_u128(session)];
        assert_eq!(details.len(), 1);
        assert!(details.iter().all(|detail| detail.contains(expected)));
    }
}

#[test]
fn absolute_vitest_json_paths_resolve_within_execution_checkout() {
    let (def, target, files) = fixture();
    let output = r#"{"testResults":[{"name":"/repo/.claude/worktrees/gate/ink/src/a.test.ts","assertionResults":[{"status":"failed","fullName":"case"}]}]}"#;
    let routed = routes(&def, &target, &files, output);
    assert_eq!(routed.owners.len(), 1);
    assert!(routed.owners.contains_key(&Uuid::from_u128(1)));
    assert!(routed.unresolved.is_empty());
}

#[test]
fn unknown_unowned_unstaffed_and_non_file_evidence_remains_unresolved() {
    let (mut def, target, files) = fixture();
    def.tasks.entry("A".into()).or_default().session = None;
    let output = "FAIL src/a.test.ts > unstaffed\nFAIL other.test.ts > unowned\nFAIL absent.test.ts > missing\nFAIL binary test_case\nFAIL /outside/ink/tests/b.test.ts > external\nFAIL ../ink/tests/b.test.ts > traversal";
    let routed = routes(&def, &target, &files, output);
    assert!(routed.owners.is_empty());
    assert_eq!(routed.unresolved.len(), 6);
    let unresolved = routed.unresolved.into_iter().collect::<Vec<_>>().join("\n");
    for evidence in [
        "unstaffed",
        "unowned",
        "missing",
        "test_case",
        "external",
        "traversal",
    ] {
        assert!(unresolved.contains(evidence), "{unresolved}");
    }
    assert!(unresolved.contains("no routable session"));
    assert!(unresolved.contains("ownership unknown"));
}

#[test]
fn ambiguous_subdirectory_paths_preserve_all_candidates_and_contact_each_owner() {
    let (mut def, target, mut files) = fixture();
    files.push("other/ink/src/a.test.ts".into());
    def.tasks
        .entry("B".into())
        .or_default()
        .files
        .push("other/ink".into());
    let routed = routes(&def, &target, &files, "FAIL ink/src/a.test.ts > ambiguous");
    assert_eq!(routed.owners.len(), 2);
    assert_eq!(routed.unresolved.len(), 2);
    assert!(
        routed
            .unresolved
            .iter()
            .all(|detail| detail.contains("ambiguous file/ownership match"))
    );
}

#[test]
fn overlapping_owners_are_all_contacted_without_guessing_first_owner() {
    let (mut def, target, files) = fixture();
    def.tasks
        .entry("B".into())
        .or_default()
        .files
        .push("ink/src".into());
    let routed = routes(&def, &target, &files, "FAIL ink/src/a.test.ts > overlap");
    assert_eq!(routed.owners.len(), 2);
    assert_eq!(routed.unresolved.len(), 1);
    assert!(
        routed
            .unresolved
            .iter()
            .all(|detail| detail.contains("ambiguous"))
    );
}

#[test]
fn explicitly_declared_shared_producer_resolves_ownership_without_authorship_claim() {
    let (mut def, target, files) = fixture();
    def.tasks
        .entry("B".into())
        .or_default()
        .files
        .push("ink/src".into());
    def.shared_files.push("ink/src".into());
    def.shared_file_producers
        .insert("ink/src".into(), "B".into());
    let routed = routes(&def, &target, &files, "FAIL ink/src/a.test.ts > shared");
    assert_eq!(routed.owners.len(), 1);
    assert!(routed.owners.contains_key(&Uuid::from_u128(2)));
    assert!(routed.unresolved.is_empty());
}

#[test]
fn multiple_tasks_in_one_session_receive_one_grouped_notice() {
    let (mut def, target, files) = fixture();
    def.tasks.entry("B".into()).or_default().session = Some(Uuid::from_u128(1).to_string());
    let routed = routes(
        &def,
        &target,
        &files,
        "FAIL ink/src/a.test.ts > one\nFAIL ink/tests/b.test.ts > two",
    );
    assert_eq!(routed.owners.len(), 1);
    assert_eq!(routed.owners[&Uuid::from_u128(1)].len(), 2);
}

#[test]
fn unrecognized_output_is_not_silently_assigned_to_any_owner() {
    let (def, target, files) = fixture();
    let routed = routes(
        &def,
        &target,
        &files,
        "process killed while writing a report",
    );
    assert!(routed.owners.is_empty());
    assert!(
        routed
            .unresolved
            .iter()
            .any(|detail| detail.contains("unparsed"))
    );
}

#[test]
fn new_files_and_pytest_identifiers_use_declared_path_boundaries() {
    let (mut def, target, mut files) = fixture();
    def.tasks
        .entry("B".into())
        .or_default()
        .new_files
        .push("tests/test_new.py".into());
    files.extend(["tests/test_new.py".into(), "ink/src-other/a.test.ts".into()]);
    let routed = routes(
        &def,
        &target,
        &files,
        "FAILED ./tests/test_new.py::case - AssertionError\nFAIL ink/src-other/a.test.ts > unrelated",
    );
    assert_eq!(routed.owners.len(), 1);
    assert!(routed.owners.contains_key(&Uuid::from_u128(2)));
    assert!(
        routed
            .unresolved
            .iter()
            .any(|detail| detail.contains("unowned file"))
    );
}

#[test]
fn noisy_inventory_routes_every_failure_before_notice_presentation_is_bounded() {
    let (def, target, files) = fixture();
    let output = (0..120)
        .map(|n| {
            let file = if n % 2 == 0 {
                "src/a.test.ts"
            } else {
                "tests/b.test.ts"
            };
            format!("FAIL {file} > case {n:03}\n")
        })
        .collect::<String>();
    let routed = routes(&def, &target, &files, &output);
    assert_eq!(routed.owners.len(), 2);
    assert!(routed.owners.values().all(|failures| failures.len() == 60));
    assert!(routed.unresolved.is_empty());
}
