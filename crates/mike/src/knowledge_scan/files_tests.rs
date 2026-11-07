use super::files::{ScanError, SourceKind, collect_excluding};
use crate::store::persistence::project_knowledge::DocumentInput;
use serde_json::json;
use std::fs;

fn collect(root: &std::path::Path, kind: SourceKind) -> Result<Vec<DocumentInput>, ScanError> {
    collect_excluding(root, kind, &[])
}

fn write(root: &std::path::Path, relative: &str, text: &str) -> anyhow::Result<()> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}

#[test]
fn collect_memory_reads_every_markdown_file_except_the_index() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    write(
        dir.path(),
        "feedback-a.md",
        "---\nname: feedback-a\ndescription: A\nmetadata:\n  type: feedback\n---\nBody A",
    )?;
    write(dir.path(), "plain.md", "# Plain heading\n\ntext")?;
    write(dir.path(), "MEMORY.md", "- [a](feedback-a.md)")?;
    write(dir.path(), "notes.txt", "ignored")?;
    let documents = collect(dir.path(), SourceKind::Memory)?;
    let paths: Vec<&str> = documents.iter().map(|d| d.relative_path.as_str()).collect();
    assert_eq!(paths, vec!["feedback-a.md", "plain.md"]);
    assert_eq!(documents[0].title, "feedback-a");
    assert_eq!(documents[0].kind, "feedback");
    assert_eq!(documents[0].status, "unknown");
    assert_eq!(documents[0].metadata["description"], json!("A"));
    assert_eq!(documents[1].title, "Plain heading");
    assert_eq!(documents[1].kind, "memory");
    Ok(())
}

#[test]
fn collect_memory_keeps_the_full_body_and_a_matching_hash() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let body = format!("---\nname: long\n---\n{}", "line of text\n".repeat(2000));
    write(dir.path(), "long.md", &body)?;
    let documents = collect(dir.path(), SourceKind::Memory)?;
    assert_eq!(documents[0].body, body);
    assert_eq!(
        documents[0].content_hash,
        super::files::hex_digest(body.as_bytes())
    );
    Ok(())
}

#[test]
fn collect_skills_reads_only_skill_files_one_level_down() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    write(
        dir.path(),
        "alpha/SKILL.md",
        "---\nname: alpha\ndescription: Does alpha\n---\nSteps",
    )?;
    write(dir.path(), "alpha/references/extra.md", "extra")?;
    write(dir.path(), "beta/SKILL.md", "no frontmatter here")?;
    write(dir.path(), "gamma/notes.md", "not a skill")?;
    let documents = collect(dir.path(), SourceKind::Skills)?;
    let paths: Vec<&str> = documents.iter().map(|d| d.relative_path.as_str()).collect();
    assert_eq!(paths, vec!["alpha/SKILL.md", "beta/SKILL.md"]);
    assert_eq!(documents[0].kind, "skill");
    assert_eq!(documents[1].title, "beta");
    Ok(())
}

#[test]
fn collect_records_a_frontmatter_problem_without_dropping_the_document() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    write(dir.path(), "odd.md", "---\nname: x\nbroken line\n---\nbody")?;
    let documents = collect(dir.path(), SourceKind::Memory)?;
    assert_eq!(documents.len(), 1);
    assert!(documents[0].metadata.get("frontmatter_error").is_some());
    Ok(())
}

#[test]
fn collect_rejects_a_file_that_is_not_utf8() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    fs::write(dir.path().join("bad.md"), [0xff, 0xfe, 0x00])?;
    let result = collect(dir.path(), SourceKind::Memory);
    assert!(matches!(result, Err(ScanError::NotUtf8 { .. })));
    Ok(())
}

#[test]
fn collect_reports_a_missing_root_as_an_io_error() {
    let result = collect(
        std::path::Path::new("/nonexistent/mike-scan-root"),
        SourceKind::Memory,
    );
    assert!(matches!(result, Err(ScanError::Io { .. })));
}
