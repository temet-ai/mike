use serde_json::json;

use super::tool_effect::{
    ChangeKind, ObservationKind, READ_BACK_KIND, ToolEffect, classify, recordable_observation_kinds,
};

fn bash(command: &str) -> ToolEffect {
    classify("Bash", &json!({ "command": command }))
}

#[test]
fn edit_tools_are_edits() {
    for tool in ["Edit", "Write", "MultiEdit", "NotebookEdit"] {
        assert_eq!(
            classify(tool, &json!({})),
            ToolEffect::Mutating(ChangeKind::Edit),
            "{tool}"
        );
    }
}

#[test]
fn bash_changes_are_classified_by_kind() {
    assert_eq!(
        bash("git commit -m x"),
        ToolEffect::Mutating(ChangeKind::Vcs)
    );
    assert_eq!(
        bash("git push origin main"),
        ToolEffect::Mutating(ChangeKind::Vcs)
    );
    assert_eq!(bash("mv a b"), ToolEffect::Mutating(ChangeKind::Fs));
    assert_eq!(
        bash("sed -i 's/a/b/' f"),
        ToolEffect::Mutating(ChangeKind::Fs)
    );
    assert_eq!(
        bash("echo hi > out.txt"),
        ToolEffect::Mutating(ChangeKind::Fs)
    );
    assert_eq!(
        bash("pnpm add left-pad"),
        ToolEffect::Mutating(ChangeKind::Deps)
    );
    assert_eq!(
        bash("./scripts/deploy-proxy.sh"),
        ToolEffect::Mutating(ChangeKind::Deploy)
    );
    assert_eq!(
        bash("gh pr create --fill"),
        ToolEffect::Mutating(ChangeKind::Deploy)
    );
    assert_eq!(
        bash("make install"),
        ToolEffect::Mutating(ChangeKind::Deploy)
    );
}

#[test]
fn a_read_only_git_command_is_not_a_change() {
    assert_eq!(
        bash(
            "git fetch -q origin main; git merge-base --is-ancestor 5de46dde origin/main && echo ok"
        ),
        ToolEffect::Neutral
    );
    assert_eq!(
        bash("git merge origin/main"),
        ToolEffect::Mutating(ChangeKind::Vcs)
    );
    assert_eq!(
        bash("git add a.rs && git commit; git status"),
        ToolEffect::Mutating(ChangeKind::Vcs)
    );
}

#[test]
fn an_angle_bracket_inside_quotes_is_not_a_redirect() {
    assert_eq!(
        bash(
            "adb devices -l; cat ~/.config/phone-control/config 2>/dev/null | sed 's/KEY=.*/KEY=<redacted>/'"
        ),
        ToolEffect::Neutral
    );
    assert_eq!(
        bash(
            "for p in \"RUN_ID=<run id>\" \"TOTAL\"; do git show origin/main:f | grep -c -F -- \"$p\"; done"
        ),
        ToolEffect::Neutral
    );
    assert_eq!(
        bash("echo \"a > b\" > out.txt"),
        ToolEffect::Mutating(ChangeKind::Fs)
    );
    assert_eq!(bash("echo \"say \\\"hi\\\" > there\""), ToolEffect::Neutral);
}

#[test]
fn a_redirect_to_dev_null_is_not_a_change() {
    assert_eq!(
        bash("cargo check 2> /dev/null"),
        ToolEffect::Observing(ObservationKind::Build)
    );
}

#[test]
fn checks_are_observations_through_the_jc02_classifier() {
    assert_eq!(
        bash("cargo nextest run"),
        ToolEffect::Observing(ObservationKind::Test)
    );
    assert_eq!(
        bash("cargo clippy -- -D warnings"),
        ToolEffect::Observing(ObservationKind::Lint)
    );
    assert_eq!(
        bash("cargo build"),
        ToolEffect::Observing(ObservationKind::Build)
    );
    assert_eq!(
        bash("scripts/gate check"),
        ToolEffect::Observing(ObservationKind::Gate)
    );
    assert_eq!(
        bash("pytest -q"),
        ToolEffect::Observing(ObservationKind::Test)
    );
}

#[test]
fn runs_http_and_deploy_polls_are_observations() {
    assert_eq!(
        bash("./target/debug/mike print-config"),
        ToolEffect::Observing(ObservationKind::Run)
    );
    assert_eq!(
        bash("node script.js"),
        ToolEffect::Observing(ObservationKind::Run)
    );
    assert_eq!(
        bash("docker logs armitage-pg"),
        ToolEffect::Observing(ObservationKind::Run)
    );
    assert_eq!(
        bash("curl -s http://localhost:9989/api/stats"),
        ToolEffect::Observing(ObservationKind::Http)
    );
    assert_eq!(
        bash("gh run watch 123"),
        ToolEffect::Observing(ObservationKind::DeployPoll)
    );
    assert_eq!(
        bash("scripts/task-wait abc"),
        ToolEffect::Observing(ObservationKind::DeployPoll)
    );
    assert_eq!(
        classify("WebFetch", &json!({"url": "https://x"})),
        ToolEffect::Observing(ObservationKind::Http)
    );
}

#[test]
fn browser_reads_and_screenshots_are_observations() {
    let nav = classify("mcp__claude-in-chrome__navigate", &json!({}));
    assert_eq!(nav, ToolEffect::Observing(ObservationKind::Browser));
    let shot = classify(
        "mcp__claude-in-chrome__computer",
        &json!({"action": "screenshot"}),
    );
    assert_eq!(shot, ToolEffect::Observing(ObservationKind::Browser));
    let click = classify(
        "mcp__claude-in-chrome__computer",
        &json!({"action": "left_click"}),
    );
    assert_eq!(click, ToolEffect::Neutral);
}

#[test]
fn reading_an_image_or_pdf_is_a_render_and_source_is_neutral() {
    assert_eq!(
        classify("Read", &json!({"file_path": "/tmp/shot.PNG"})),
        ToolEffect::Observing(ObservationKind::Render)
    );
    assert_eq!(
        classify("Read", &json!({"file_path": "/tmp/report.pdf"})),
        ToolEffect::Observing(ObservationKind::Render)
    );
    assert_eq!(
        classify("Read", &json!({"file_path": "src/lib.rs"})),
        ToolEffect::Neutral
    );
}

#[test]
fn everything_else_is_neutral() {
    for tool in [
        "Grep",
        "Glob",
        "ToolSearch",
        "mcp__mike__send_session_message",
    ] {
        assert_eq!(classify(tool, &json!({})), ToolEffect::Neutral, "{tool}");
    }
    assert_eq!(bash("ls -la"), ToolEffect::Neutral);
}

const CONSTRAINT: &str = "turn_facts_observation_kind_check";

fn newest_definition_of_the_constraint() -> anyhow::Result<String> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../mike-base/src/store/persistence/migrations");
    let mut definers = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("sql") {
            continue;
        }
        let text = std::fs::read_to_string(&path)?;
        if text.contains(&format!("ADD CONSTRAINT {CONSTRAINT}")) {
            definers.push((path, text));
        }
    }
    definers.sort();
    let (_, text) = definers
        .pop()
        .ok_or_else(|| anyhow::anyhow!("no migration defines {CONSTRAINT}"))?;
    Ok(text)
}

fn allowed_kinds(migration: &str) -> anyhow::Result<Vec<String>> {
    let code: String = migration
        .lines()
        .filter(|line| !line.trim_start().starts_with("--"))
        .collect::<Vec<_>>()
        .join("\n");
    let after_constraint = code
        .split(&format!("ADD CONSTRAINT {CONSTRAINT}"))
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("constraint definition not found"))?;
    let list = after_constraint
        .split("observation_kind IN (")
        .nth(1)
        .and_then(|rest| rest.split(')').next())
        .ok_or_else(|| anyhow::anyhow!("observation_kind IN (...) list not found"))?;
    Ok(list
        .split(',')
        .map(|item| item.trim().trim_matches('\'').to_owned())
        .collect())
}

#[test]
fn newest_migration_allows_exactly_the_kinds_the_code_can_write() -> anyhow::Result<()> {
    let mut in_sql = allowed_kinds(&newest_definition_of_the_constraint()?)?;
    let mut in_code: Vec<String> = recordable_observation_kinds()
        .into_iter()
        .map(str::to_owned)
        .collect();
    in_sql.sort();
    in_code.sort();
    assert_eq!(
        in_sql, in_code,
        "turn_facts_observation_kind_check and the Rust list of observation kinds differ; add a migration that re-adds the constraint"
    );
    Ok(())
}

#[test]
fn allowed_kinds_parser_reads_the_list_and_skips_comments() -> anyhow::Result<()> {
    let sql = "-- observation_kind IN ('nope')\nALTER TABLE t ADD CONSTRAINT turn_facts_observation_kind_check\n CHECK (observation_kind IS NULL\n OR observation_kind IN ('a', 'b',\n 'c'));";
    assert_eq!(allowed_kinds(sql)?, ["a", "b", "c"]);
    assert!(allowed_kinds("SELECT 1").is_err());
    Ok(())
}

#[test]
fn recordable_kinds_are_the_nine_codes_plus_read_back() {
    let kinds = recordable_observation_kinds();
    assert_eq!(kinds.len(), 10);
    assert_eq!(kinds.last().copied(), Some(READ_BACK_KIND));
    assert!(kinds.contains(&ObservationKind::DeployPoll.code()));
}

#[test]
fn all_lists_every_observation_kind() {
    let mut codes = Vec::new();
    for kind in ObservationKind::ALL {
        match kind {
            ObservationKind::Test
            | ObservationKind::Build
            | ObservationKind::Lint
            | ObservationKind::Gate
            | ObservationKind::Run
            | ObservationKind::Http
            | ObservationKind::Browser
            | ObservationKind::Render
            | ObservationKind::DeployPoll => codes.push(kind.code()),
        }
    }
    codes.sort_unstable();
    codes.dedup();
    assert_eq!(codes.len(), ObservationKind::ALL.len());
}
