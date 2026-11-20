use serde_json::{Value, json};

use super::finding::{as_json, validate};

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
    let validated =
        validate(established().to_string().as_bytes(), repo.path()).map_err(anyhow::Error::msg)?;
    let finding = validated.finding;
    assert!(validated.followups.is_empty());
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
    let finding = validate(doc.to_string().as_bytes(), repo.path())
        .map_err(anyhow::Error::msg)?
        .finding;
    assert!(!finding.cause_established);
    assert!(finding.confirmed_cause.is_none());
    Ok(())
}

#[test]
fn investigate_finding_with_a_confirmed_cause_and_no_suspected_one_takes_the_confirmed()
-> anyhow::Result<()> {
    let repo = repo()?;
    let finding = validate(&with("suspected_cause", json!("")), repo.path())
        .map_err(anyhow::Error::msg)?
        .finding;
    assert_eq!(
        finding.suspected_cause,
        "logs.rs reads past the 8 MiB bound"
    );
    let mut doc = established();
    doc["suspected_cause"] = json!("");
    doc["cause_established"] = json!(false);
    doc["confirmed_cause"] = Value::Null;
    doc["owning_part"] = Value::Null;
    let refused = validate(doc.to_string().as_bytes(), repo.path()).err();
    assert_eq!(refused.as_deref(), Some("suspected_cause is empty"));
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

fn followup(kind: &str) -> Value {
    json!({
        "kind": kind,
        "title": "Bound the log read",
        "body": "logs.rs reads the whole file",
        "owning_part": "crates/mike/src/wright/mod.rs",
        "same_as": null,
    })
}

#[test]
fn investigate_finding_followups_are_parsed_in_order() -> anyhow::Result<()> {
    let repo = repo()?;
    let mut second = followup("check_tuning");
    second["owning_part"] = Value::Null;
    second["same_as"] = json!(7);
    let bytes = with("followups", json!([followup("code_defect"), second]));
    let validated = validate(&bytes, repo.path()).map_err(anyhow::Error::msg)?;
    assert_eq!(validated.followups.len(), 2);
    assert_eq!(validated.followups[0].kind, "code_defect");
    assert_eq!(
        validated.followups[0].owning_part.as_deref(),
        Some("crates/mike/src/wright/mod.rs")
    );
    assert_eq!(validated.followups[1].owning_part, None);
    assert_eq!(validated.followups[1].same_as, Some(7));
    let echoed = as_json(&validated);
    assert_eq!(echoed["followups"].as_array().map(Vec::len), Some(2));
    Ok(())
}

#[test]
fn investigate_finding_bad_followups_are_refused_with_the_position() -> anyhow::Result<()> {
    let repo = repo()?;
    let mut blank_title = followup("hardening");
    blank_title["title"] = json!("  ");
    let mut long_title = followup("hardening");
    long_title["title"] = json!("x".repeat(201));
    let mut blank_body = followup("hardening");
    blank_body["body"] = json!("");
    let mut outside = followup("hardening");
    outside["owning_part"] = json!("../outside");
    let mut missing = followup("hardening");
    missing["owning_part"] = json!("crates/none.rs");
    let mut extra = followup("hardening");
    extra["priority"] = json!(1);
    let eleven: Vec<Value> = (0..11).map(|_| followup("hardening")).collect();
    let cases = [
        (
            json!([followup("code_defect"), followup("wish")]),
            "followups[1].kind is `wish`",
        ),
        (json!([blank_title]), "followups[0].title is empty"),
        (json!([long_title]), "followups[0].title is 201 characters"),
        (json!([blank_body]), "followups[0].body is empty"),
        (json!([outside]), "followups[0]: owning_part"),
        (json!([missing]), "does not exist"),
        (json!([extra]), "line 1, column"),
        (Value::Array(eleven), "at most 10"),
    ];
    for (followups, expected) in cases {
        let Err(reason) = validate(&with("followups", followups), repo.path()) else {
            anyhow::bail!("accepted a bad follow-up, expected `{expected}`");
        };
        assert!(reason.contains(expected), "`{reason}` lacks `{expected}`");
    }
    Ok(())
}

#[test]
fn investigate_finding_same_cause_as_is_sorted_and_deduplicated() -> anyhow::Result<()> {
    let repo = repo()?;
    let bytes = with("same_cause_as", json!([9, 4, 9]));
    let validated = validate(&bytes, repo.path()).map_err(anyhow::Error::msg)?;
    assert_eq!(validated.same_cause_as, vec![4, 9]);
    assert_eq!(as_json(&validated)["same_cause_as"], json!([4, 9]));
    Ok(())
}

#[test]
fn investigate_finding_same_cause_as_without_an_established_cause_is_refused() -> anyhow::Result<()>
{
    let repo = repo()?;
    let mut doc = established();
    doc["cause_established"] = json!(false);
    doc["confirmed_cause"] = Value::Null;
    doc["same_cause_as"] = json!([3]);
    let Err(reason) = validate(doc.to_string().as_bytes(), repo.path()) else {
        anyhow::bail!("merged causes on an unconfirmed cause");
    };
    assert!(reason.contains("same_cause_as"));
    Ok(())
}
