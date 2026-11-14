use serde_json::json;

use super::*;
use crate::doctor::code_scan::{CodeScanFacts, check, probe_check};

fn report_body(rules_version: serde_json::Value) -> serde_json::Value {
    json!({
        "scanning": true,
        "delivering": true,
        "pushing": false,
        "grammars": ["bash", "rust"],
        "rules_version": rules_version,
        "rule_ids": ["a", "b", "c"],
        "summary": {"scans": 12, "last_scan_at": "2026-11-14T09:00:00Z"},
    })
}

#[test]
fn the_api_body_becomes_facts_with_the_rule_count_and_version() -> anyhow::Result<()> {
    let facts = CodeScanFacts::from_json(&report_body(json!("abc123")))
        .ok_or_else(|| anyhow::anyhow!("the body did not parse"))?;
    assert!(facts.scanning && facts.delivering && !facts.pushing);
    assert_eq!(facts.grammars, vec!["bash", "rust"]);
    assert_eq!(
        facts.rules.map(|r| (r.count, r.version)),
        Some((3, "abc123".to_string()))
    );
    assert_eq!(facts.scans_24h, 12);
    assert_eq!(facts.last_scan_at.as_deref(), Some("2026-11-14T09:00:00Z"));
    Ok(())
}

#[test]
fn a_body_without_the_scan_fields_gives_no_facts() {
    assert_eq!(CodeScanFacts::from_json(&json!({"error": "no"})), None);
}

#[test]
fn the_check_is_informational_and_names_switches_grammars_rules_and_last_scan() -> anyhow::Result<()>
{
    let checks = evaluate(&healthy());
    let found = check_of(&checks)?;
    assert_eq!(found.status, Status::Ok);
    for part in [
        "change record and scan on",
        "telling sessions off",
        "push to idle sessions off",
        "2 grammars (bash, rust)",
        "42 built-in rules, version abc123",
        "last scan 2026-11-14T09:00:00Z",
        "12 scans in 24 h",
    ] {
        assert!(found.detail.contains(part), "{part}: {}", found.detail);
    }
    Ok(())
}

fn check_of(checks: &[Check]) -> anyhow::Result<&Check> {
    checks
        .iter()
        .find(|c| c.id == "code_scan")
        .ok_or_else(|| anyhow::anyhow!("no code_scan check"))
}

#[test]
fn rules_that_did_not_load_fail_the_check() -> anyhow::Result<()> {
    let facts = CodeScanFacts::from_json(&report_body(serde_json::Value::Null))
        .ok_or_else(|| anyhow::anyhow!("the body did not parse"))?;
    assert_eq!(check(Some(&facts)).status, Status::Fail);
    let probe = serde_json::to_value(probe_check(Some(&facts)))?;
    assert_eq!(probe["status"], "fail");
    assert_eq!(probe["required"], true);
    assert_eq!(probe["code"], "CODE_SCAN_RULES_NOT_LOADED");
    Ok(())
}

#[test]
fn an_unanswering_proxy_leaves_the_check_informational() -> anyhow::Result<()> {
    assert_eq!(check(None).status, Status::Ok);
    let probe = serde_json::to_value(probe_check(None))?;
    assert_eq!(probe["required"], false);
    assert_eq!(probe["status"], "unavailable");
    Ok(())
}
