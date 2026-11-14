use serde_json::{Value, json};

use super::finding::validate;

fn repo() -> anyhow::Result<tempfile::TempDir> {
    let tmp = tempfile::tempdir()?;
    std::fs::create_dir_all(tmp.path().join("crates/mike/src/wright"))?;
    std::fs::write(tmp.path().join("crates/mike/src/wright/mod.rs"), "")?;
    Ok(tmp)
}

fn established() -> Value {
    json!({
        "symptom": "the watch worker logs a timeout every pass",
        "suspected_cause": "the log probe reads the whole file",
        "confirmed_cause": "logs.rs reads past the 8 MiB bound",
        "cause_established": true,
        "owning_part": "crates/mike/src/wright/mod.rs",
        "kind": "code_defect",
        "remedy": "bound the read",
        "evidence": [{"kind": "file", "ref": "crates/mike/src/wright/mod.rs"}, {"kind": "episode", "ref": "23"}],
    })
}

fn with(field: &str, value: Value) -> Vec<u8> {
    let mut doc = established();
    doc[field] = value;
    doc.to_string().into_bytes()
}

#[test]
fn investigate_finding_with_cause_and_part_is_accepted() -> anyhow::Result<()> {
    let repo = repo()?;
    let finding =
        validate(established().to_string().as_bytes(), repo.path()).map_err(anyhow::Error::msg)?;
    assert!(finding.cause_established);
    assert_eq!(
        finding.owning_part.as_deref(),
        Some("crates/mike/src/wright/mod.rs")
    );
    assert_eq!(finding.evidence.as_array().map(Vec::len), Some(2));
    Ok(())
}

#[test]
fn investigate_finding_without_established_cause_says_so() -> anyhow::Result<()> {
    let repo = repo()?;
    let mut doc = established();
    doc["cause_established"] = json!(false);
    doc["confirmed_cause"] = Value::Null;
    doc["owning_part"] = Value::Null;
    doc["kind"] = json!("unknown");
    let finding = validate(doc.to_string().as_bytes(), repo.path()).map_err(anyhow::Error::msg)?;
    assert!(!finding.cause_established);
    assert!(finding.confirmed_cause.is_none());
    Ok(())
}

#[test]
fn investigate_finding_breaking_the_contract_is_refused_with_the_reason() -> anyhow::Result<()> {
    let repo = repo()?;
    let cases = [
        (
            with("confirmed_cause", Value::Null),
            "confirmed_cause is missing",
        ),
        (with("owning_part", Value::Null), "owning_part is missing"),
        (
            with("owning_part", json!("../outside")),
            "inside the repository",
        ),
        (
            with("owning_part", json!("/etc/passwd")),
            "inside the repository",
        ),
        (
            with("owning_part", json!("crates/none.rs")),
            "does not exist",
        ),
        (with("evidence", json!([])), "needs 1 to"),
        (
            with("evidence", json!([{"kind": "wish", "ref": "x"}])),
            "evidence[0].kind",
        ),
        (with("kind", json!("guess")), "kind is `guess`"),
        (with("symptom", json!("  ")), "symptom is empty"),
        (with("extra", json!(1)), "line 1, column"),
        (b"not json".to_vec(), "line 1, column"),
    ];
    for (bytes, expected) in cases {
        let Err(reason) = validate(&bytes, repo.path()) else {
            anyhow::bail!("accepted {}", String::from_utf8_lossy(&bytes));
        };
        assert!(reason.contains(expected), "`{reason}` lacks `{expected}`");
    }
    let mut doc = established();
    doc["cause_established"] = json!(false);
    let Err(reason) = validate(doc.to_string().as_bytes(), repo.path()) else {
        anyhow::bail!("accepted a confirmed cause that was not established");
    };
    assert!(reason.contains("cause_established is false"));
    Ok(())
}
