use super::{Change, Evidence, evidence};

fn change(files: &str, added: &str) -> Change {
    let diff: String = added.lines().map(|line| format!("+{line}\n")).collect();
    Change::from_diff(files, &format!("+++ b/x\n{diff}"))
}

fn deploy_tooling() -> Change {
    change(
        "scripts/deploy-proxy.sh\nscripts/test-fleet-deploy-guard",
        "deploy_guard_tick() {\n  run_fleet_guard \"$@\"\n}",
    )
}

#[test]
fn a_table_name_in_the_failure_outranks_a_path_overlap() {
    let changes = [
        deploy_tooling(),
        change(
            "crates/mike/src/store/persistence/migrations/20261022000000_launch_fleet_events.sql",
            "CREATE TABLE fleet_events (\n  id bigserial PRIMARY KEY\n);",
        ),
    ];
    let output = "running scripts/deploy-proxy.sh\n\
        thread 'launch::migration' panicked at crates/mike/tests/integration/store/launch/migration.rs:42:5:\n\
        expected fleet_events to be absent before the migration";
    assert_eq!(
        evidence(&changes, output),
        vec![(1, Evidence::Defines("fleet_events".to_string()))]
    );
}

#[test]
fn a_compile_error_names_the_carrying_commit_and_the_defining_commit() {
    let changes = [
        deploy_tooling(),
        change(
            "crates/mike/src/store/persistence/fleet/run_record.rs",
            "pub struct InterventionMeasure {\n    pub label: String,\n}",
        ),
        change(
            "crates/mike/src/store/persistence/fleet/mod.rs",
            "use run_record::InterventionMeasure;\nlet measure = InterventionMeasure::default();",
        ),
    ];
    let output = "error[E0412]: cannot find type `InterventionMeasure` in module `run_record`\n \
        --> crates/mike/src/store/persistence/fleet/mod.rs:88:17\n";
    assert_eq!(
        evidence(&changes, output),
        vec![
            (1, Evidence::Defines("InterventionMeasure".to_string())),
            (
                2,
                Evidence::Located("crates/mike/src/store/persistence/fleet/mod.rs".to_string())
            ),
        ]
    );
}

#[test]
fn a_name_two_commits_add_names_neither_by_itself() {
    let changes = [
        change("a.rs", "let shared_name = 1;"),
        change("b.rs", "let shared_name = 2;"),
    ];
    assert!(evidence(&changes, "assertion failed: shared_name == 3").is_empty());
}

#[test]
fn with_no_name_matched_a_path_overlap_still_names_the_first_commit() {
    let changes = [change("ok.txt", "ok"), change("bad.txt", "bad")];
    assert_eq!(
        evidence(&changes, "bad.txt: broken"),
        vec![(1, Evidence::Touches("bad.txt".to_string()))]
    );
}

#[test]
fn nothing_matched_names_no_suspect() {
    let changes = [deploy_tooling()];
    assert!(evidence(&changes, "test result: FAILED. 1 failed").is_empty());
}
