use anyhow::{Context, Result};

use super::{Kind, WiringFinding, WiringRules, check, counts, introduced};
use crate::SourceFile;

const RULES: &str = include_str!("../../../../.mike/structural/wiring.yml");

fn rules() -> Result<WiringRules> {
    WiringRules::from_yaml(RULES).context("the repo's wiring rules do not load")
}

fn file(path: &str, content: &str) -> SourceFile {
    SourceFile {
        path: path.to_owned(),
        content: content.to_owned(),
    }
}

fn names(findings: &[WiringFinding], kind: Kind) -> Vec<&str> {
    findings
        .iter()
        .filter(|finding| finding.kind == kind)
        .map(|finding| finding.name.as_str())
        .collect()
}

fn run(files: &[SourceFile]) -> Result<Vec<WiringFinding>> {
    Ok(check(&rules()?, files))
}

#[test]
fn the_repo_rules_load_with_one_rule_per_kind() -> Result<()> {
    let rules = rules()?;
    assert_eq!(rules.ids().len(), Kind::ALL.len(), "{:?}", rules.ids());
    Ok(())
}

#[test]
fn a_pub_fn_nothing_calls_is_a_finding() -> Result<()> {
    let found = run(&[file(
        "crates/demo/src/lib.rs",
        "pub fn orphan() {}\npub(crate) fn hidden() {}\nfn main() {}\n",
    )])?;
    assert_eq!(names(&found, Kind::PubFnNoCaller), vec!["orphan"]);
    Ok(())
}

#[test]
fn a_pub_fn_with_a_production_caller_is_not_a_finding() -> Result<()> {
    let found = run(&[
        file("crates/demo/src/lib.rs", "pub fn adopted() {}\n"),
        file(
            "crates/demo/src/main.rs",
            "fn main() { demo::adopted(); }\n",
        ),
    ])?;
    assert!(names(&found, Kind::PubFnNoCaller).is_empty(), "{found:?}");
    Ok(())
}

#[test]
fn a_caller_inside_a_test_module_does_not_count() -> Result<()> {
    let found = run(&[file(
        "crates/demo/src/lib.rs",
        "pub fn only_tested() {}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() { super::only_tested(); }\n}\n",
    )])?;
    assert_eq!(names(&found, Kind::PubFnNoCaller), vec!["only_tested"]);
    Ok(())
}

#[test]
fn a_caller_in_a_test_file_does_not_count() -> Result<()> {
    let found = run(&[
        file("crates/demo/src/lib.rs", "pub fn only_tested() {}\n"),
        file(
            "crates/demo/src/lib_tests.rs",
            "fn t() { crate::only_tested(); }\n",
        ),
    ])?;
    assert_eq!(names(&found, Kind::PubFnNoCaller), vec!["only_tested"]);
    Ok(())
}

#[test]
fn recursion_and_a_use_declaration_are_not_callers() -> Result<()> {
    let found = run(&[
        file(
            "crates/demo/src/lib.rs",
            "pub fn loops(n: u32) { if n > 0 { loops(n - 1) } }\n",
        ),
        file("crates/demo/src/other.rs", "use crate::loops;\n"),
    ])?;
    assert_eq!(names(&found, Kind::PubFnNoCaller), vec!["loops"]);
    Ok(())
}

#[test]
fn a_method_call_and_a_call_inside_a_macro_are_callers() -> Result<()> {
    let found = run(&[file(
        "crates/demo/src/lib.rs",
        "impl A { pub fn go(&self) {} pub fn later(&self) {} }\nfn run(a: A) { a.go(); vec![a.later()]; }\n",
    )])?;
    assert!(names(&found, Kind::PubFnNoCaller).is_empty(), "{found:?}");
    Ok(())
}

#[test]
fn a_request_stage_nothing_constructs_is_a_finding() -> Result<()> {
    let stage = "pub struct Lost;\nimpl RequestStage for Lost {}\npub struct Wired;\nimpl RequestStage for Wired {}\n";
    let registry =
        "fn registry() -> Vec<Box<dyn RequestStage>> { vec![Box::new(crate::a::Wired)] }\n";
    let found = run(&[
        file("crates/demo/src/stage.rs", stage),
        file("crates/demo/src/pipeline.rs", registry),
    ])?;
    assert_eq!(names(&found, Kind::StageUnregistered), vec!["Lost"]);
    Ok(())
}

#[test]
fn a_spawn_fn_nothing_calls_is_a_worker_finding_and_not_a_pub_fn_finding() -> Result<()> {
    let found = run(&[
        file(
            "crates/demo/src/worker.rs",
            "pub fn spawn_lost(s: &S) {}\npub fn spawn_started(s: &S) {}\n",
        ),
        file(
            "crates/demo/src/workers.rs",
            "fn start(s: &S) { crate::worker::spawn_started(s); }\n",
        ),
    ])?;
    assert_eq!(names(&found, Kind::WorkerUnspawned), vec!["spawn_lost"]);
    assert!(names(&found, Kind::PubFnNoCaller).is_empty(), "{found:?}");
    Ok(())
}

const META: &str = "pub const READ_KEY: &str = \"demo.read\";\nfn entries() -> Vec<SettingMeta> {\n    vec![\n        SettingMeta { key: \"demo.unread\", group: \"g\" },\n        SettingMeta { key: READ_KEY, group: \"g\" },\n        SettingMeta { key: \"demo.in_spa\", group: \"g\" },\n    ]\n}\n";

#[test]
fn a_setting_nothing_reads_is_a_finding_and_each_read_form_counts() -> Result<()> {
    let found = run(&[
        file("crates/mike-base/src/settings/meta_demo.rs", META),
        file(
            "crates/demo/src/worker.rs",
            "fn tick(s: &S) { s.get(READ_KEY); }\n",
        ),
        file(
            "frontend/src/lib/panel.ts",
            "export const k = 'demo.in_spa';\n",
        ),
    ])?;
    assert_eq!(names(&found, Kind::SettingUnread), vec!["demo.unread"]);
    Ok(())
}

const MANIFEST: &str = "pub(super) const ROUTES: &[RouteEntry] = &[\n    route(\"GET\", \"/api/lost\", C),\n    route(\"GET\", \"/api/things/{id}\", C),\n    route(\"GET\", \"/api/prefixed/{id}/deep\", C),\n];\nfn register(r: Router) -> Router { r.route(\"/api/lost\", get(h)) }\n";

#[test]
fn a_route_nothing_calls_is_a_finding_and_template_and_prefix_calls_count() -> Result<()> {
    let found = run(&[
        file("crates/demo/src/api/manifest/things.rs", MANIFEST),
        file(
            "frontend/src/lib/api.ts",
            "const a = `/api/things/${encodeURIComponent(id)}`;\nconst b = `/api/prefixed/${id}/deep`;\n// GET /api/lost\n",
        ),
    ])?;
    assert_eq!(names(&found, Kind::RouteUnconsumed), vec!["/api/lost"]);
    Ok(())
}

#[test]
fn introduced_reports_only_findings_the_later_tree_adds() -> Result<()> {
    let base = run(&[file("crates/demo/src/lib.rs", "pub fn old() {}\n")])?;
    let head = run(&[file(
        "crates/demo/src/lib.rs",
        "pub fn old() {}\npub fn fresh() {}\n",
    )])?;
    let added = introduced(&base, &head);
    assert_eq!(names(&added, Kind::PubFnNoCaller), vec!["fresh"]);
    assert!(introduced(&head, &head).is_empty());
    Ok(())
}

#[test]
fn counts_names_every_kind_even_when_zero() -> Result<()> {
    let found = run(&[file("crates/demo/src/lib.rs", "pub fn orphan() {}\n")])?;
    let counted = counts(&found);
    assert_eq!(counted.len(), Kind::ALL.len());
    assert_eq!(counted.get(&Kind::PubFnNoCaller), Some(&1));
    assert_eq!(counted.get(&Kind::RouteUnconsumed), Some(&0));
    Ok(())
}

#[test]
fn a_rule_that_does_not_parse_names_the_rule() -> Result<()> {
    let broken = "rules:\n  - id: bad\n    kind: pub_fn_no_caller\n    message: m\n    declare:\n      ast: { language: rust, rule: { kind: function_item } }\n    uses:\n      - scope: [crates/]\n";
    let err = WiringRules::from_yaml(broken)
        .err()
        .context("a use with neither ast nor text must be refused")?;
    assert!(err.to_string().contains("bad"), "{err}");
    Ok(())
}
