use std::collections::HashSet;

use serde_json::json;

use super::commit_modules::{Unfiled, declared, module_files, unfiled, warning};
use super::definition::FleetDefinition;
use super::tests::parsed;

fn tree(files: &[&str]) -> HashSet<String> {
    files.iter().map(|f| (*f).to_string()).collect()
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

#[test]
fn every_top_level_module_declaration_is_found_whatever_its_visibility() {
    let source = "\
mod a;
pub mod b;
pub(crate) mod c;
pub(super) mod d;
pub(in crate::x) mod e;
#[cfg(test)]
mod f_tests;
#[cfg(test)] mod g_tests;
mod h; mod i;
";
    assert_eq!(
        declared(source),
        strings(&["a", "b", "c", "d", "e", "f_tests", "g_tests", "h", "i"])
    );
}

#[test]
fn inline_nested_commented_and_redirected_modules_are_not_declarations() {
    let lines = [
        "mod inline {",
        "    mod nested;",
        "}",
        "// mod commented;",
        "use crate::module;",
        "pub fn model() {}",
        "#[path = \"elsewhere.rs\"]",
        "mod redirected;",
        "#[path = \"elsewhere.rs\"] mod same_line;",
        "mod kept;",
        "pubmod nope;",
        "mod 9bad;",
    ];
    let source = lines.join("\n");
    assert_eq!(declared(&source), strings(&["kept"]));
    assert!(declared("").is_empty());
}

#[test]
fn a_module_file_sits_beside_a_mod_lib_or_main_file_and_below_any_other() {
    let flat = |file| module_files(file, "mine");
    assert_eq!(
        flat("crates/mike/src/api/changes/mod.rs"),
        [
            "crates/mike/src/api/changes/mine.rs".to_string(),
            "crates/mike/src/api/changes/mine/mod.rs".to_string()
        ]
    );
    assert_eq!(flat("src/lib.rs")[0], "src/mine.rs");
    assert_eq!(flat("src/main.rs")[1], "src/mine/mod.rs");
    assert_eq!(flat("lib.rs")[0], "mine.rs");
    assert_eq!(
        flat("crates/mike/src/launch/fleet.rs"),
        [
            "crates/mike/src/launch/fleet/mine.rs".to_string(),
            "crates/mike/src/launch/fleet/mine/mod.rs".to_string()
        ]
    );
    assert_eq!(flat("top.rs")[0], "top/mine.rs");
}

#[test]
fn a_declaration_added_without_its_file_in_the_tree_is_named() {
    let file = "src/api/changes/mod.rs";
    let after = "mod mine;\nmod theirs;\n";
    let found = unfiled(
        file,
        Some("mod theirs;\n"),
        after,
        &tree(&["src/api/changes/theirs.rs"]),
    );
    assert_eq!(
        found,
        vec![Unfiled {
            declared_in: file.to_string(),
            module: "mine".to_string(),
            expected: [
                "src/api/changes/mine.rs".to_string(),
                "src/api/changes/mine/mod.rs".to_string()
            ],
        }]
    );
}

#[test]
fn a_module_file_in_the_tree_either_way_satisfies_the_declaration() {
    let file = "src/mod.rs";
    assert!(unfiled(file, None, "mod a;\n", &tree(&["src/a.rs"])).is_empty());
    assert!(unfiled(file, None, "mod a;\n", &tree(&["src/a/mod.rs"])).is_empty());
    assert_eq!(
        unfiled(file, None, "mod a;\n", &tree(&["src/a/b.rs"])).len(),
        1
    );
}

#[test]
fn a_declaration_already_in_the_parent_is_not_this_commits_to_answer_for() {
    let file = "src/mod.rs";
    assert!(unfiled(file, Some("mod old;\n"), "mod old;\n", &tree(&[])).is_empty());
    assert_eq!(unfiled(file, None, "mod old;\n", &tree(&[])).len(), 1);
    assert!(unfiled(file, Some("mod old;\n"), "", &tree(&[])).is_empty());
}

fn fleet() -> FleetDefinition {
    let mut value = super::tests::complete();
    value["tasks"] = json!({
        "T1": {"name": "list", "discipline": "rust-developer", "criteria": ["C1"],
               "files": ["src/api/changes/mod.rs"], "seat": "rust-T1"},
        "T2": {"name": "mine", "discipline": "rust-developer", "criteria": ["C1"],
               "files": ["src/api/changes/other.rs"],
               "new_files": ["src/api/changes/mine.rs"], "seat": "rust-T2"}
    });
    parsed(&value)
}

#[test]
fn the_warning_names_the_missing_files_and_the_task_that_lists_them() {
    let def = fleet();
    let missing = &unfiled("src/api/changes/mod.rs", None, "mod mine;\n", &tree(&[]))[0];
    let said = warning(&def, missing);
    assert!(
        said.starts_with("src/api/changes/mod.rs declares `mod mine;` but neither src/api/changes/mine.rs nor src/api/changes/mine/mod.rs is in this commit or in the tree it lands on"),
        "{said}"
    );
    assert!(said.contains("Task T2 (mine) lists it"), "{said}");
}

#[test]
fn the_warning_for_a_file_no_task_lists_says_only_what_is_missing() {
    let def = fleet();
    let missing = &unfiled("src/elsewhere/mod.rs", None, "mod stray;\n", &tree(&[]))[0];
    let said = warning(&def, missing);
    assert!(said.contains("src/elsewhere/stray.rs"), "{said}");
    assert!(!said.contains("lists it"), "{said}");
}
