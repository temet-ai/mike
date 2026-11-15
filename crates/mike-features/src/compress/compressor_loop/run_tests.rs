use anyhow::Result;
use serde_json::json;

use super::decide::Settings;
use super::run::read_evidence;
use super::settings_api::{IMPLEMENTATION, policy_from};

#[test]
fn read_evidence_defaults_to_the_saved_qualification_file() -> Result<()> {
    let state = tempfile::tempdir()?;
    assert_eq!(read_evidence(None, state.path())?, None);
    std::fs::write(
        state.path().join("qualification.json"),
        json!({"verdict": "qualified"}).to_string(),
    )?;
    assert_eq!(
        read_evidence(None, state.path())?,
        Some(json!({"verdict": "qualified"}))
    );
    let other = state.path().join("other.json");
    std::fs::write(&other, json!({"verdict": "x"}).to_string())?;
    assert_eq!(
        read_evidence(Some(&other), state.path())?,
        Some(json!({"verdict": "x"}))
    );
    Ok(())
}

#[test]
fn read_evidence_names_the_file_when_it_is_missing_or_not_json() -> Result<()> {
    let state = tempfile::tempdir()?;
    let missing = state.path().join("absent.json");
    let error = read_evidence(Some(&missing), state.path()).err();
    assert!(error.is_some_and(|e| e.to_string().contains("absent.json")));
    let broken = state.path().join("broken.json");
    std::fs::write(&broken, "{not json")?;
    let error = read_evidence(Some(&broken), state.path()).err();
    assert!(error.is_some_and(|e| e.to_string().contains("broken.json")));
    Ok(())
}

#[test]
fn policy_from_carries_the_live_file_read_settings_and_the_implementation() {
    let settings = Settings(
        [
            ("file_read_compression.min_bytes", json!(4096)),
            ("file_read_compression.protected_paths", json!(["**/*.env"])),
            ("file_read_compression.forward_share", json!(50)),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect(),
    );
    let policy = policy_from(&settings, Some("abc123".to_string()));
    assert_eq!(policy["min_bytes"], json!(4096));
    assert_eq!(policy["head_lines"], json!(null));
    assert_eq!(policy["protected_paths"], json!(["**/*.env"]));
    assert_eq!(policy["implementation"], json!(IMPLEMENTATION));
    assert_eq!(policy["build_sha"], json!("abc123"));
    assert!(policy.get("forward_share").is_none());
    assert_eq!(policy_from(&settings, None)["build_sha"], json!(null));
}
