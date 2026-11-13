use super::*;
use crate::lesson_proposer::skill_author::TriggerQuery;

fn files(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| (*n).to_owned()).collect()
}

fn material(skill_md: &str) -> Material {
    Material {
        name: "database-migrations".to_owned(),
        description: "Ship a migration safely.".to_owned(),
        skill_md: skill_md.to_owned(),
        references: Vec::new(),
        queries: vec![TriggerQuery {
            query: "look at the prompts page".to_owned(),
            should_trigger: true,
        }],
        lesson_ids: Vec::new(),
        agent_id: None,
    }
}

#[test]
fn files_named_by_a_path_in_the_text_come_first() {
    let tracked = files(&[
        "crates/mike/src/api/skills/mod.rs",
        "crates/mike/src/store/mod.rs",
        "docs/SCHEMA.md",
    ]);
    let chosen = relevant(&tracked, "see crates/mike/src/api/skills");
    assert_eq!(chosen, files(&["crates/mike/src/api/skills/mod.rs"]));
}

#[test]
fn a_file_name_in_the_text_matches_that_file_anywhere_in_the_tree() {
    let tracked = files(&["a/PromptEditor.svelte", "b/Other.svelte", "README.md"]);
    let chosen = relevant(&tracked, "edit PromptEditor.svelte next");
    assert_eq!(chosen, files(&["a/PromptEditor.svelte"]));
}

#[test]
fn a_word_in_the_text_matches_a_path_component() {
    let tracked = files(&["frontend/src/routes/prompts/+page.svelte", "docs/INDEX.md"]);
    let chosen = relevant(&tracked, "the prompts page shows anomalies");
    assert_eq!(chosen, files(&["frontend/src/routes/prompts/+page.svelte"]));
}

#[test]
fn a_word_that_matches_too_many_files_is_ignored() {
    let tracked: Vec<String> = (0..120).map(|i| format!("tests/case_{i}.rs")).collect();
    assert!(relevant(&tracked, "every tests file").is_empty());
}

#[test]
fn the_selection_is_capped_and_sorted_by_path() {
    let tracked: Vec<String> = (0..70).map(|i| format!("pages/page_{i:02}.md")).collect();
    let chosen = relevant(&tracked, "pages/page_");
    assert_eq!(chosen.len(), 60);
    assert!(chosen.windows(2).all(|pair| pair[0] <= pair[1]));
}

#[test]
fn top_level_collapses_directories_and_keeps_root_files() {
    let tracked = files(&["src/a.rs", "src/b.rs", "docs/x.md", "Cargo.toml"]);
    assert_eq!(top_level(&tracked), files(&["Cargo.toml", "docs/", "src/"]));
}

#[test]
fn context_for_a_listed_project_names_the_path_the_files_and_the_tool_file() {
    let project = Workspace {
        path: "/work/mike".to_owned(),
        available: true,
        tracked: 812,
        top_level: files(&["crates/", "docs/"]),
        files: files(&["docs/SCHEMA.md"]),
    };
    let context = context(&project);
    assert!(context.contains("/work/mike"));
    assert!(context.contains("812 tracked files"));
    assert!(context.contains("not part of the project"));
    assert!(context.contains("Top level: crates/, docs/"));
    assert!(context.contains("- docs/SCHEMA.md\n"));
}

#[test]
fn context_for_an_unlistable_project_still_names_it_without_inventing_files() {
    let project = Workspace {
        path: "/work/mike".to_owned(),
        ..Workspace::default()
    };
    let context = context(&project);
    assert!(context.contains("/work/mike"));
    assert!(context.contains("cannot be listed"));
    assert!(!context.contains("- "));
}

#[tokio::test]
async fn load_without_a_checkout_is_unavailable() {
    let missing = load("/definitely/not/a/checkout", &material("x")).await;
    assert!(!missing.available);
    assert_eq!(missing.path, "/definitely/not/a/checkout");
    assert!(missing.files.is_empty());
    assert!(!load("", &material("x")).await.available);
}

#[tokio::test]
async fn load_lists_tracked_files_of_a_checkout() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let root = dir.path();
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(root.join("src/numbering.rs"), "fn main() {}")?;
    std::fs::write(root.join("README.md"), "readme")?;
    for args in [["init", "-q"], ["add", "."]] {
        let status = tokio::process::Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .status()
            .await?;
        anyhow::ensure!(status.success(), "git {args:?} failed");
    }
    let loaded = load(
        &root.to_string_lossy(),
        &material("Edit src/numbering.rs first."),
    )
    .await;
    assert!(loaded.available);
    assert_eq!(loaded.tracked, 2);
    assert_eq!(loaded.top_level, files(&["README.md", "src/"]));
    assert_eq!(loaded.files, files(&["src/numbering.rs"]));
    Ok(())
}
