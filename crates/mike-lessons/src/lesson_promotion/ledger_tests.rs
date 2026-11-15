use std::path::Path;

use super::super::test_support::{key, project_uuid, today};
use super::*;

fn live_row(folder: &str, loads: i64, judged: i64, corrected: i64) -> LiveRow {
    LiveRow {
        project_uuid: project_uuid(),
        folder: folder.to_owned(),
        window_start: DateTime::<Utc>::default(),
        window_end: DateTime::<Utc>::default(),
        loads,
        load_sessions: 3,
        loaded_turns: judged + 1,
        judged_turns: judged,
        corrected_turns: corrected,
        base_judged_turns: 40,
        base_corrected_turns: 8,
    }
}

fn entry(first: &str) -> LedgerEntry {
    LedgerEntry {
        first_emitted: first.to_owned(),
        lesson_watermark: 2,
        ..LedgerEntry::default()
    }
}

#[test]
fn a_skill_is_corrected_above_base_only_with_ten_loads_and_a_higher_rate() {
    assert!(corrected_above_base(&live_row("x", 10, 8, 4)));
    assert!(!corrected_above_base(&live_row("x", 9, 8, 8)));
    assert!(!corrected_above_base(&live_row("x", 30, 10, 2)));
    assert!(!corrected_above_base(&live_row("x", 30, 10, 1)));
    assert!(!corrected_above_base(&live_row("x", 30, 0, 0)));
    let mut no_base = live_row("x", 30, 5, 5);
    no_base.base_judged_turns = 0;
    no_base.base_corrected_turns = 0;
    assert!(!corrected_above_base(&no_base));
}

#[test]
fn measure_asks_only_for_skills_the_ledger_owns_and_names_them_safely() {
    let good = key("deploy-and-restart");
    let ledger: Ledger = [
        (good.clone(), entry("2026-10-01")),
        (
            format!("{}:x'); DROP TABLE y; --", project_uuid()),
            entry("2026-10-01"),
        ),
        ("not-a-uuid:fine".to_owned(), entry("2026-10-01")),
        (
            format!(
                "{}:late",
                crate::lesson_promotion::test_support::other_uuid()
            ),
            entry("yesterday"),
        ),
    ]
    .into_iter()
    .collect();
    let keys: Vec<String> = ledger.keys().cloned().collect();
    let found = shipped_skills(&ledger, &keys);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].folder, "deploy-and-restart");
    assert_eq!(found[0].project, project_uuid());
    assert_eq!(found[0].since.to_string(), "2026-10-01");
    assert!(shipped_skills(&Ledger::new(), &[]).is_empty());
}

#[test]
fn retire_unloaded_drops_only_stale_unloaded_topic_skills() {
    let stale = (today() - chrono::Duration::days(31)).to_string();
    let fresh = (today() - chrono::Duration::days(10)).to_string();
    let mut ledger: Ledger = [
        (key("stale"), entry(&stale)),
        (key("fresh"), entry(&fresh)),
        (key("loaded"), entry(&stale)),
        (key("vanished"), entry(&stale)),
    ]
    .into_iter()
    .collect();
    let emitted: Emitted = ["stale", "fresh", "loaded"]
        .iter()
        .map(|folder| {
            (
                key(folder),
                Path::new("/p/.claude/skills").join(folder).join("SKILL.md"),
            )
        })
        .collect();
    let mut desired: BTreeMap<String, String> = emitted
        .values()
        .map(|path| (path.display().to_string(), "x".to_owned()))
        .collect();
    desired.insert(
        "/h/skills/shared-workflows/SKILL.md".to_owned(),
        "shared".to_owned(),
    );
    let loads = BTreeMap::from([(key("loaded"), 1)]);
    let retired = retire_unloaded(&mut desired, &mut ledger, &loads, today(), &emitted);
    assert_eq!(retired, vec![key("stale")]);
    assert!(!desired.contains_key("/p/.claude/skills/stale/SKILL.md"));
    assert!(desired.contains_key("/p/.claude/skills/fresh/SKILL.md"));
    assert!(desired.contains_key("/p/.claude/skills/loaded/SKILL.md"));
    assert!(desired.contains_key("/h/skills/shared-workflows/SKILL.md"));
    assert_eq!(
        ledger[&key("stale")].retired_at.as_deref(),
        Some(today().to_string().as_str())
    );
    assert_eq!(
        ledger[&key("stale")].retired_reason.as_deref(),
        Some("unloaded")
    );
    assert!(ledger[&key("fresh")].retired_at.is_none());
    assert!(ledger[&key("vanished")].retired_at.is_none());
}

#[test]
fn hold_marks_only_an_active_entry() {
    let mut ledger: Ledger = [(key("a"), entry("2026-10-01"))].into_iter().collect();
    hold(&mut ledger, &key("a"), "user", today());
    assert_eq!(ledger[&key("a")].retired_reason.as_deref(), Some("user"));
    hold(&mut ledger, &key("a"), "policy", today());
    assert_eq!(ledger[&key("a")].retired_reason.as_deref(), Some("user"));
    hold(&mut ledger, &key("missing"), "user", today());
    assert!(!ledger.contains_key(&key("missing")));
}

#[test]
fn recording_a_measure_keeps_it_unpublished_with_the_retirement_it_saw() -> anyhow::Result<()> {
    let mut ledger: Ledger = [(key("deploy"), entry("2026-10-01"))].into_iter().collect();
    let measured: Measured = [(
        key("deploy"),
        Live {
            row: live_row("deploy", 12, 8, 4),
            first_shipped_on: NaiveDate::from_ymd_opt(2026, 10, 1).unwrap_or_default(),
        },
    )]
    .into();
    record(&mut ledger, &measured);
    assert_eq!(unpublished(&ledger).len(), 1);
    let first: SkillLiveOutcome = serde_json::from_value(
        ledger[&key("deploy")]
            .outcome
            .clone()
            .ok_or_else(|| anyhow::anyhow!("an outcome must be recorded"))?,
    )?;
    assert_eq!((first.loads, first.corrected_turns), (12, 4));
    assert!(first.retired_at.is_none() && first.retired_reason.is_none());
    ledger
        .entry(key("deploy"))
        .and_modify(|found| mark_retired(found, today(), "corrected_above_base"));
    record(&mut ledger, &measured);
    let second: SkillLiveOutcome = serde_json::from_value(
        ledger[&key("deploy")]
            .outcome
            .clone()
            .ok_or_else(|| anyhow::anyhow!("an outcome must be recorded"))?,
    )?;
    assert_eq!(
        second.retired_reason,
        Some(RetiredReason::CorrectedAboveBase)
    );
    assert!(second.retired_at.is_some());
    Ok(())
}

#[test]
fn only_unpublished_outcomes_are_waiting() {
    let mut sent = entry("2026-10-01");
    sent.outcome = Some(serde_json::json!({ "folder": "two" }));
    sent.published = true;
    let mut waiting = entry("2026-10-01");
    waiting.outcome = Some(serde_json::json!({ "folder": "one" }));
    let ledger: Ledger = [
        ("a:1".to_owned(), waiting),
        ("a:2".to_owned(), sent),
        ("a:3".to_owned(), entry("2026-10-01")),
    ]
    .into_iter()
    .collect();
    let found = unpublished(&ledger);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, "a:1");
}

#[test]
fn a_ledger_written_by_the_python_promoter_is_read_back() -> anyhow::Result<()> {
    let text = r#"{"abc:deploy": {"first_emitted": "2026-10-01", "retired_at": null,
        "lesson_watermark": 9, "path": "/p/SKILL.md", "draft_id": 3, "retired_reason": null,
        "outcome": {"folder": "deploy"}, "published": false}}"#;
    let ledger: Ledger = serde_json::from_str(text)?;
    assert_eq!(ledger["abc:deploy"].lesson_watermark, 9);
    assert_eq!(ledger["abc:deploy"].draft_id, Some(3));
    Ok(())
}
