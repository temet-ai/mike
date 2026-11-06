use serde_json::json;

use super::checks::{Missing, check_definition};
use super::checks_files::absent_task_files;
use super::environment::{Environment, check_environment};
use super::tests::{changed, complete};

const THREE_RUNS: &str = "the menu, options and effects steps pass three consecutive SHOT: PASS runs of menu.json, options.json and effects.json";
const RERUN_ALL: &str =
    "The acceptance seat reruns at the run's last commit every steps file this run touched";
const PICTURE_EXPIRES: &str = "A picture proves a slice only if it was taken at or after the seat's last commit touching what it shows";

fn about(found: &[Missing], item: &str) -> Vec<String> {
    found
        .iter()
        .filter(|m| m.item == item)
        .map(|m| m.problem.clone())
        .collect()
}

#[test]
fn a_criterion_asking_for_three_passing_runs_in_a_row_is_missing_with_the_sentence_quoted() {
    let def = changed(json!({"criteria": {"C6": {"text": THREE_RUNS, "check": "one run"}}}));
    let found = about(&check_definition(&def), "criteria.C6.text");
    assert_eq!(found.len(), 1);
    assert!(found[0].contains(THREE_RUNS), "{}", found[0]);
}

#[test]
fn a_criterion_asking_for_everything_to_run_again_at_the_last_commit_is_missing() {
    let def = changed(json!({"criteria": {"C8": {"text": RERUN_ALL, "check": "read the diff"}}}));
    let found = about(&check_definition(&def), "criteria.C8.text");
    assert_eq!(found.len(), 1);
    assert!(found[0].contains(RERUN_ALL), "{}", found[0]);
}

#[test]
fn a_criterion_whose_evidence_stops_counting_at_a_later_commit_is_missing() {
    let def = changed(json!({"criteria": {"C8": {"text": "shown", "check": PICTURE_EXPIRES}}}));
    assert_eq!(about(&check_definition(&def), "criteria.C8.check").len(), 1);
}

#[test]
fn a_sentence_that_requires_the_repetition_is_missing_even_when_it_says_never() {
    for sentence in [
        "Never mark the slice done unless its test passes three times in a row",
        "A seat must not report until the test passes three consecutive runs",
        "Ask for the test to pass three consecutive runs",
    ] {
        let def = changed(json!({"criteria": {"C6": {"text": sentence, "check": "one run"}}}));
        let found = about(&check_definition(&def), "criteria.C6.text");
        assert_eq!(found.len(), 1, "{sentence}");
        assert!(found[0].contains(sentence), "{}", found[0]);
    }
}

#[test]
fn a_prohibition_of_the_repetition_passes() {
    for sentence in [
        "A criterion never asks for a run to pass more than once in a row",
        "The author must not ask for everything to be run again at the last commit",
    ] {
        let def = changed(json!({"criteria": {"C1": {"text": sentence, "check": "one run"}}}));
        assert_eq!(check_definition(&def), vec![], "{sentence}");
    }
}

#[test]
fn a_task_brief_asking_for_repeated_passes_is_missing() {
    let def = changed(
        json!({"tasks": {"P2": {"brief": "Build the page. Make effects.json pass three runs in a row"}}}),
    );
    let found = about(&check_definition(&def), "tasks.P2.brief");
    assert_eq!(found.len(), 1);
    assert!(
        found[0].contains("pass three runs in a row"),
        "{}",
        found[0]
    );
}

#[test]
fn sentences_that_do_not_ask_for_repetition_pass() {
    for sentence in [
        "run the test once",
        "three plans are shown",
        "a timed-out test is rerun alone once",
        "the check passes at the newest commit",
        "three tests exist for the parser",
        "In core-game-3 the gate was first asked 2 hours 4 minutes in and none of its 8 attempts was green",
        "mike lists as missing a criterion, check or brief that asks for a test or a run to pass more than once in a row, for everything to be run again at the last commit, or for evidence that stops counting at any later commit",
        "Fixtures, from fleet core-game-3: \"three consecutive SHOT: PASS runs of menu.json\"; the sentences that must pass include \"run the test once\"",
    ] {
        let def = changed(json!({"criteria": {"C1": {"text": sentence, "check": sentence}}}));
        assert_eq!(check_definition(&def), vec![], "{sentence}");
    }
}

#[test]
fn goals_within_the_limit_and_with_known_placeholders_pass() {
    let def = changed(json!({"goals": {
        "workers": "task {task} ({name}) is reported with fleet_report and the check passed at its commit: {criteria}; or stop after 30 turns"
    }, "tasks": {"P2": {"goal": "the page renders both fixtures, shown by one test run"}}}));
    assert_eq!(check_definition(&def), vec![]);
}

#[test]
fn a_goal_over_4000_characters_once_filled_in_is_missing() {
    let long = "x".repeat(3_990);
    let def = changed(json!({"goals": {
        "workers": format!("{long} {{criteria}}")
    }}));
    let found = check_definition(&def);
    let worker = about(&found, "goals.workers (task P1)");
    assert_eq!(worker.len(), 1, "{found:?}");
    assert!(worker[0].contains("at most 4000"), "{}", worker[0]);
}

#[test]
fn a_control_seat_goal_left_in_an_old_definition_is_not_checked() {
    let def =
        changed(json!({"goals": {"fleet": "z".repeat(4_500), "coordinator": "y".repeat(4_500)}}));
    assert_eq!(check_definition(&def), vec![]);
}

#[test]
fn a_worker_goal_with_an_unknown_placeholder_is_missing() {
    let def = changed(json!({"tasks": {"P2": {"goal": "task {task} lands {files} green"}}}));
    let found = about(&check_definition(&def), "tasks.P2.goal");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("uses {files}"), "{}", found[0]);
}

fn task_with(files: serde_json::Value, new_files: serde_json::Value) -> serde_json::Value {
    json!({"tasks": {"P2": {"files": files, "new_files": new_files}}})
}

fn problems_for_files(patch: serde_json::Value, repo: &std::path::Path) -> Vec<Missing> {
    let mut value = complete();
    super::definition::merge(&mut value, &json!({"repo": repo.to_string_lossy()}));
    super::definition::merge(&mut value, &patch);
    let def = super::tests::parsed(&value);
    let env = Environment {
        repo_exists: true,
        absent_files: absent_task_files(&def, repo),
        ..Environment::default()
    };
    check_environment(&def, &env)
        .into_iter()
        .filter(|m| m.item == "tasks.P2.files")
        .collect()
}

#[test]
fn a_task_file_that_is_not_in_the_repository_is_missing() -> anyhow::Result<()> {
    let repo = tempfile::tempdir()?;
    std::fs::create_dir_all(repo.path().join("crates/mike/src/launch/fleet"))?;
    std::fs::write(repo.path().join("crates/mike/src/launch/fleet/gate.rs"), "")?;
    let patch = task_with(
        json!([
            "crates/mike/src/launch/fleet/gate.rs",
            "crates/mike/src/fleet/commit/"
        ]),
        json!([]),
    );
    let found = problems_for_files(patch, repo.path());
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].item, "tasks.P2.files");
    assert!(found[0].problem.contains("crates/mike/src/fleet/commit"));
    Ok(())
}

#[test]
fn a_task_file_marked_new_is_not_missing() -> anyhow::Result<()> {
    let repo = tempfile::tempdir()?;
    std::fs::create_dir_all(repo.path().join("src/glob"))?;
    let patch = task_with(
        json!(["src/gate_queue.rs", "src/glob/**"]),
        json!(["src/gate_queue.rs"]),
    );
    assert_eq!(problems_for_files(patch, repo.path()), vec![]);
    Ok(())
}

#[test]
fn the_files_are_not_read_when_the_repo_is_not_a_folder() {
    let def = changed(task_with(json!(["nowhere/at/all.rs"]), json!([])));
    let env = Environment {
        repo_exists: false,
        absent_files: ["nowhere/at/all.rs".to_string()].into(),
        ..Environment::default()
    };
    let found = check_environment(&def, &env);
    assert!(
        found.iter().all(|m| !m.item.starts_with("tasks.")),
        "{found:?}"
    );
}

#[test]
fn a_task_owning_a_folder_that_holds_another_tasks_file_is_listed_with_both_named() {
    let def = changed(json!({"tasks": {
        "P1": {"files": ["crates/mike/src/fleet/"]},
        "P2": {"files": ["crates/mike/src/fleet/commit.rs"]}
    }}));
    let found = check_definition(&def);
    let hit = found
        .iter()
        .find(|m| m.item == "tasks.P1.files" && m.problem.contains("task P2"))
        .map(|m| m.problem.clone());
    let problem = hit.unwrap_or_default();
    assert!(problem.contains("crates/mike/src/fleet"), "{found:?}");
    assert!(
        problem.contains("crates/mike/src/fleet/commit.rs"),
        "{found:?}"
    );
}

const LATE_CHECK: &str = "the P11 and P12 seats each used it on two models";

fn late_definition(serving: &[&str]) -> super::definition::FleetDefinition {
    changed(json!({
        "criteria": {"C3": {"text": "the command exists and is used", "check": LATE_CHECK}},
        "tasks": {
            "P11": {"name": "bay", "discipline": "db-dev", "criteria": ["C1"],
                    "files": ["bay/"], "brief": "Build it.", "after": ["P1"]},
            "P12": {"name": "lab", "discipline": "db-dev", "criteria": ["C1"],
                    "files": ["lab/"], "brief": "Build it.", "after": ["P1", "P11"]},
            "P1": {"criteria": serving}
        }
    }))
}

#[test]
fn a_criterion_only_later_tasks_can_satisfy_is_missing_on_the_task_it_is_served_by() {
    let found = about(
        &check_definition(&late_definition(&["C1", "C3"])),
        "criteria.C3.check",
    );
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found.iter().any(|p| p.contains("task P11")), "{found:?}");
    assert!(found.iter().any(|p| p.contains("task P12")), "{found:?}");
    assert!(
        found[0].contains("only starts after P1 has passed"),
        "{found:?}"
    );
}

#[test]
fn a_criterion_also_served_by_the_last_task_is_not_circular() {
    let mut value = complete();
    super::definition::merge(
        &mut value,
        &json!({
            "criteria": {"C3": {"text": "used", "check": LATE_CHECK}},
            "tasks": {
                "P11": {"name": "bay", "discipline": "db-dev", "criteria": ["C1"],
                        "files": ["bay/"], "brief": "Build it.", "after": ["P1"]},
                "P12": {"name": "lab", "discipline": "db-dev", "criteria": ["C3"],
                        "files": ["lab/"], "brief": "Build it.", "after": ["P1", "P11"]}
            }
        }),
    );
    let def = super::tests::parsed(&value);
    assert_eq!(
        about(&check_definition(&def), "criteria.C3.check"),
        Vec::<String>::new()
    );
}

#[test]
fn a_check_naming_a_task_by_a_longer_id_is_not_read_as_naming_the_shorter() {
    let mut def = late_definition(&["C1", "C3"]);
    def.criteria.insert(
        "C3".into(),
        super::definition::Criterion {
            text: "x".into(),
            check: "the P111 seat used it".into(),
            ..super::definition::Criterion::default()
        },
    );
    assert_eq!(
        about(&check_definition(&def), "criteria.C3.check"),
        Vec::<String>::new()
    );
}

#[test]
fn a_task_waiting_on_more_than_three_tasks_is_missing_and_three_pass() {
    let mut def = changed(json!({}));
    let first = def.tasks.keys().next().cloned().unwrap_or_default();
    let item = format!("tasks.{first}.after");
    if let Some(task) = def.tasks.get_mut(&first) {
        task.after = ["A", "B", "C"].map(String::from).to_vec();
    }
    let waits = |def: &super::definition::FleetDefinition| -> Vec<String> {
        about(&check_definition(def), &item)
            .into_iter()
            .filter(|problem| problem.starts_with("waits for"))
            .collect()
    };
    assert_eq!(waits(&def), Vec::<String>::new());
    if let Some(task) = def.tasks.get_mut(&first) {
        task.after.push("D".into());
    }
    let found = waits(&def);
    assert_eq!(found.len(), 1);
    assert!(found[0].contains("waits for 4 tasks (A, B, C, D)"));
}

fn chained(length: usize) -> super::definition::FleetDefinition {
    let mut tasks = serde_json::Map::new();
    for n in 1..=length {
        let mut task = json!({"name": format!("step {n}"), "discipline": "db-dev", "criteria": ["C1"],
            "files": [format!("step{n}/")], "brief": "Build it."});
        if n > 1 {
            task["after"] = json!([format!("S{}", n - 1)]);
        }
        tasks.insert(format!("S{n}"), task);
    }
    changed(json!({"tasks": tasks}))
}

#[test]
fn a_chain_of_four_tasks_is_missing_with_the_chain_drawn_and_three_pass() {
    let found = about(&check_definition(&chained(4)), "tasks.S4.after");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].starts_with("S1 → S2 → S3 → S4 is a chain of 4 tasks"),
        "{}",
        found[0]
    );
    assert_eq!(
        about(&check_definition(&chained(3)), "tasks.S3.after"),
        Vec::<String>::new()
    );
}

#[test]
fn a_chain_lint_survives_a_cycle_and_an_unknown_task() {
    let def = changed(json!({"tasks": {
        "P1": {"after": ["P10", "Q9"]},
        "P2": {"after": ["P1"]}
    }}));
    let found: Vec<Missing> = check_definition(&def)
        .into_iter()
        .filter(|m| m.problem.contains("is a chain of"))
        .collect();
    assert!(found.is_empty(), "{found:?}");
}
