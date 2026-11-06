use serde_json::{Value, json};

use super::super::atomic::{Edited, edit_object, read_object, write_text};
use super::super::error::ClaudeCodeError;

#[test]
fn editing_a_missing_file_creates_it_with_the_change() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("nested").join("settings.json");
    let (edited, ()) = edit_object(&path, |root| {
        root.insert("a".to_string(), json!(1));
        Ok(())
    })?;
    assert_eq!(edited, Edited::Written);
    assert_eq!(read_object(&path)?.get("a"), Some(&json!(1)));
    Ok(())
}

#[test]
fn editing_keeps_every_other_key_and_nested_value() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join(".claude.json");
    let original = json!({
        "numStartups": 12,
        "projects": {"/work": {"hasTrustDialogAccepted": true, "allowedTools": ["Bash"]}},
        "mcpServers": {"other": {"command": "x"}},
        "float": 1.5,
        "nothing": null
    });
    std::fs::write(&path, serde_json::to_string(&original)?)?;
    edit_object(&path, |root| {
        root.insert("added".to_string(), json!(true));
        Ok(())
    })?;
    let after: Value = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
    let mut expected = original;
    expected["added"] = json!(true);
    assert_eq!(after, expected);
    Ok(())
}

#[test]
fn a_change_that_changes_nothing_does_not_rewrite_the_file() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("a.json");
    let odd_layout = "{\"b\":1,   \"a\":2}";
    std::fs::write(&path, odd_layout)?;
    let (edited, ()) = edit_object(&path, |_| Ok(()))?;
    assert_eq!(edited, Edited::Unchanged);
    assert_eq!(std::fs::read_to_string(&path)?, odd_layout);
    Ok(())
}

#[test]
fn invalid_json_is_reported_and_left_untouched() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("a.json");
    std::fs::write(&path, "{ not json")?;
    let error = edit_object(&path, |root| {
        root.insert("a".to_string(), json!(1));
        Ok(())
    })
    .err()
    .ok_or_else(|| anyhow::anyhow!("expected an error"))?;
    assert!(matches!(error, ClaudeCodeError::Parse { .. }), "{error}");
    assert_eq!(std::fs::read_to_string(&path)?, "{ not json");
    Ok(())
}

#[test]
fn a_json_array_at_the_top_is_a_shape_error() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("a.json");
    std::fs::write(&path, "[1,2]")?;
    let error = read_object(&path)
        .err()
        .ok_or_else(|| anyhow::anyhow!("expected an error"))?;
    assert!(error.to_string().contains("a list"), "{error}");
    Ok(())
}

#[cfg(unix)]
#[test]
fn writing_keeps_the_mode_of_an_existing_file_and_follows_a_symlink() -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir()?;
    let real = dir.path().join("real.json");
    let link = dir.path().join("link.json");
    std::fs::write(&real, "{}")?;
    std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o640))?;
    std::os::unix::fs::symlink(&real, &link)?;
    write_text(&link, "{\"a\":1}\n", 0o600)?;
    assert!(std::fs::symlink_metadata(&link)?.file_type().is_symlink());
    assert_eq!(std::fs::read_to_string(&real)?, "{\"a\":1}\n");
    assert_eq!(
        std::fs::metadata(&real)?.permissions().mode() & 0o777,
        0o640
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_new_file_gets_the_default_mode() -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir()?;
    let path = dir.path().join("new.json");
    write_text(&path, "{}", 0o600)?;
    assert_eq!(
        std::fs::metadata(&path)?.permissions().mode() & 0o777,
        0o600
    );
    Ok(())
}
