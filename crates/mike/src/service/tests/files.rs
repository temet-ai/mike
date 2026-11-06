use crate::service::error::ServiceError;
use crate::service::files::{Existing, MARKER, Presence, Written, inspect, is_ours, remove, write};

fn marked(body: &str) -> String {
    format!("# {MARKER}.\n{body}\n")
}

#[test]
fn only_text_carrying_the_marker_near_the_top_counts_as_ours() {
    assert!(is_ours(&marked("body")));
    assert!(!is_ours("[Unit]\nDescription=someone else's unit\n"));
    let buried = format!("{}# {MARKER}\n", "line\n".repeat(30));
    assert!(!is_ours(&buried));
}

#[test]
fn inspect_tells_missing_ours_and_foreign_files_apart() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let ours = dir.path().join("ours");
    let foreign = dir.path().join("foreign");
    std::fs::write(&ours, marked("a"))?;
    std::fs::write(&foreign, "mine, not mike's")?;
    assert_eq!(inspect(&dir.path().join("missing"))?, Existing::Missing);
    assert_eq!(inspect(&ours)?.presence(), Presence::Ours);
    assert_eq!(inspect(&foreign)?, Existing::Foreign);
    Ok(())
}

#[test]
fn a_file_that_is_not_text_counts_as_foreign() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let binary = dir.path().join("binary");
    std::fs::write(&binary, [0xff_u8, 0xfe, 0x00, 0x80])?;
    assert_eq!(inspect(&binary)?, Existing::Foreign);
    Ok(())
}

#[test]
fn write_creates_parents_replaces_changes_and_skips_identical_content() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("a/b/unit");
    let first = marked("one");
    assert_eq!(write(&path, &first, &Existing::Missing)?, Written::Created);
    assert_eq!(std::fs::read_to_string(&path)?, first);
    assert_eq!(write(&path, &first, &inspect(&path)?)?, Written::Unchanged);
    let second = marked("two");
    assert_eq!(write(&path, &second, &inspect(&path)?)?, Written::Replaced);
    assert_eq!(std::fs::read_to_string(&path)?, second);
    Ok(())
}

#[test]
fn write_refuses_to_replace_a_file_mike_did_not_write() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("unit");
    std::fs::write(&path, "someone else's")?;
    let result = write(&path, &marked("x"), &inspect(&path)?);
    assert!(matches!(result, Err(ServiceError::ForeignFile { .. })));
    assert_eq!(std::fs::read_to_string(&path)?, "someone else's");
    Ok(())
}

#[test]
fn remove_deletes_the_file_and_reports_a_missing_one() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("unit");
    std::fs::write(&path, marked("x"))?;
    remove(&path)?;
    assert!(!path.exists());
    assert!(matches!(remove(&path), Err(ServiceError::Remove { .. })));
    Ok(())
}
