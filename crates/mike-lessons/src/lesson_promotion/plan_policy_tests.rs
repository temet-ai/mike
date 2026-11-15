use std::fs;

use super::super::ledger::{Live, LiveRow, Measured};
use super::super::test_support::{Case, Home, draft, key, lessons, project_uuid, today};
use super::*;

const DEPLOY: &str = "deploy-and-restart";

fn pair() -> Vec<LessonRow> {
    lessons(DEPLOY, 1..3)
}

fn skill_path(home: &Home, folder: &str) -> PathBuf {
    home.skills().join(folder).join("SKILL.md")
}

fn read(path: &Path) -> anyhow::Result<String> {
    Ok(fs::read_to_string(path)?)
}

fn measured(folder: &str, loads: i64, judged: i64, corrected: i64) -> Measured {
    let row = LiveRow {
        project_uuid: project_uuid(),
        folder: folder.to_owned(),
        window_start: chrono::DateTime::default(),
        window_end: chrono::DateTime::default(),
        loads,
        load_sessions: 3,
        loaded_turns: judged + 1,
        judged_turns: judged,
        corrected_turns: corrected,
        base_judged_turns: 40,
        base_corrected_turns: 8,
    };
    Measured::from([(
        key(folder),
        Live {
            row,
            first_shipped_on: today(),
        },
    )])
}

fn loaded(folder: &str) -> BTreeMap<String, i64> {
    BTreeMap::from([(key(folder), 1)])
}

fn old_entry(days: i64) -> LedgerEntry {
    LedgerEntry {
        first_emitted: (today() - chrono::Duration::days(days)).to_string(),
        lesson_watermark: 2,
        ..LedgerEntry::default()
    }
}

fn three_topics() -> Vec<LessonRow> {
    let mut rows = lessons("alpha", 1..3);
    rows.extend(lessons("bravo", 3..5));
    rows.extend(lessons("charlie", 5..7));
    rows
}

#[test]
fn a_new_skill_over_the_cap_retires_the_skill_with_the_least_evidence() -> anyhow::Result<()> {
    let mut home = Home::new()?;
    home.ledger.insert(key("alpha"), old_entry(20));
    home.ledger.insert(key("bravo"), old_entry(20));
    let mut case = Case::new(three_topics());
    case.loads = BTreeMap::from([(key("alpha"), 9), (key("bravo"), 1)]);
    case.cap = 2;
    let outcome = home.run(case)?;
    assert_eq!(outcome.settled.capped, vec![key("bravo")]);
    assert!(outcome.settled.held_for_cap.is_empty());
    assert_eq!(
        home.skill_names(),
        vec!["alpha".to_owned(), "charlie".to_owned()]
    );
    assert!(home.ledger[&key("bravo")].retired_at.is_some());
    Ok(())
}

#[test]
fn a_new_skill_is_held_when_every_existing_skill_is_younger_than_a_week() -> anyhow::Result<()> {
    let mut home = Home::new()?;
    home.ledger.insert(key("alpha"), old_entry(3));
    home.ledger.insert(key("bravo"), old_entry(3));
    let mut case = Case::new(three_topics());
    case.cap = 2;
    case.loads = BTreeMap::from([(key("alpha"), 1), (key("bravo"), 1)]);
    let outcome = home.run(case)?;
    assert!(outcome.settled.capped.is_empty());
    assert_eq!(outcome.settled.held_for_cap, vec![key("charlie")]);
    assert_eq!(
        home.skill_names(),
        vec!["alpha".to_owned(), "bravo".to_owned()]
    );
    Ok(())
}

#[test]
fn an_ask_project_without_a_policy_writes_nothing_until_a_draft_is_approved() -> anyhow::Result<()>
{
    let mut home = Home::new()?;
    let mut case = Case::new(pair());
    case.policies.clear();
    case.drafts = vec![draft(5, None, 2, "Drafted.")];
    let outcome = home.run(case)?;
    assert!(!skill_path(&home, DEPLOY).exists());
    assert!(outcome.plan.emitted.is_empty());
    assert_eq!(outcome.plan.held_for_approval, vec![key(DEPLOY)]);

    let mut case = Case::new(pair());
    case.policies.clear();
    case.drafts = vec![draft(5, Some("approved"), 2, "Approved.")];
    let outcome = home.run(case)?;
    assert!(read(&skill_path(&home, DEPLOY))?.contains("Approved."));
    assert_eq!(outcome.settled.authored, 1);
    let entry = &home.ledger[&key(DEPLOY)];
    assert_eq!(entry.draft_id, Some(5));
    assert_eq!(
        entry.path.as_deref(),
        Some(skill_path(&home, DEPLOY).display().to_string().as_str())
    );
    Ok(())
}

#[test]
fn switching_to_ask_removes_a_skill_written_without_approval() -> anyhow::Result<()> {
    let mut home = Home::new()?;
    home.run(Case::new(pair()))?;
    assert!(skill_path(&home, DEPLOY).exists());
    let mut ask = Case::new(pair());
    ask.policies = BTreeMap::from([(project_uuid().to_string(), "ask".to_owned())]);
    home.run(ask)?;
    assert!(!skill_path(&home, DEPLOY).exists());
    assert_eq!(
        home.ledger[&key(DEPLOY)].retired_reason.as_deref(),
        Some("policy")
    );
    home.run(Case::new(pair()))?;
    assert!(skill_path(&home, DEPLOY).exists());
    Ok(())
}

#[test]
fn a_folder_the_user_retired_is_removed_and_not_written_until_restored() -> anyhow::Result<()> {
    let mut home = Home::new()?;
    let approved = || vec![draft(5, Some("approved"), 2, "Approved.")];
    let mut first = Case::new(pair());
    first.drafts = approved();
    home.run(first)?;
    assert!(skill_path(&home, DEPLOY).exists());
    let mut retired = Case::new(lessons(DEPLOY, 1..4));
    retired.drafts = approved();
    retired.user_retired = BTreeSet::from([key(DEPLOY)]);
    let outcome = home.run(retired)?;
    assert!(!skill_path(&home, DEPLOY).exists());
    assert_eq!(outcome.plan.held_by_user, vec![key(DEPLOY)]);
    let mut restored = Case::new(pair());
    restored.drafts = approved();
    home.run(restored)?;
    assert!(skill_path(&home, DEPLOY).exists());
    Ok(())
}

#[test]
fn an_auto_project_writes_nothing_for_a_folder_whose_newest_draft_the_user_rejected()
-> anyhow::Result<()> {
    let mut home = Home::new()?;
    let mut shipped = Case::new(pair());
    shipped.drafts = vec![draft(5, None, 2, "Shipped.")];
    home.run(shipped)?;
    assert!(read(&skill_path(&home, DEPLOY))?.contains("Shipped."));
    let mut rejected = Case::new(pair());
    rejected.drafts = vec![draft(5, Some("rejected"), 2, "Shipped.")];
    let outcome = home.run(rejected)?;
    assert!(!skill_path(&home, DEPLOY).exists());
    assert_eq!(outcome.plan.held_by_user, vec![key(DEPLOY)]);
    assert_eq!(
        home.ledger[&key(DEPLOY)].retired_reason.as_deref(),
        Some("user")
    );
    Ok(())
}

#[test]
fn an_accepted_draft_authors_the_skill_and_its_references() -> anyhow::Result<()> {
    let mut home = Home::new()?;
    let mut authored = draft(5, None, 2, "Drafted body.");
    authored.reference_files = serde_json::json!([
        {"name": "restarting", "when_to_read": "you restart.", "content": "# Restarting\n"},
        {"name": "lessons", "when_to_read": "x", "content": "overwrite"},
        {"name": "../escape", "when_to_read": "x", "content": "escape"},
    ]);
    let expected = authored.skill_md.clone();
    let mut case = Case::new(pair());
    case.drafts = vec![authored];
    let outcome = home.run(case)?;
    let dir = home.skills().join(DEPLOY);
    assert_eq!(read(&dir.join("SKILL.md"))?, expected);
    assert_eq!(
        read(&dir.join("references/restarting.md"))?,
        "# Restarting\n"
    );
    assert_ne!(read(&dir.join("references/lessons.md"))?, "overwrite");
    assert!(!dir.join("escape.md").exists());
    assert_eq!(outcome.settled.authored, 1);
    Ok(())
}

#[test]
fn a_draft_older_than_the_newest_lesson_keeps_shipping_with_every_lesson_in_its_reference()
-> anyhow::Result<()> {
    let mut home = Home::new()?;
    let mut case = Case::new(pair());
    case.drafts = vec![draft(5, None, 1, "older draft")];
    let outcome = home.run(case)?;
    assert!(read(&skill_path(&home, DEPLOY))?.contains("older draft"));
    let reference = read(&home.skills().join(DEPLOY).join("references/lessons.md"))?;
    assert_eq!(reference.matches("## [lesson:").count(), 2);
    assert_eq!(outcome.settled.authored, 1);
    Ok(())
}

#[test]
fn a_retired_topic_skill_stays_retired_until_it_gains_a_lesson() -> anyhow::Result<()> {
    let mut home = Home::new()?;
    assert_eq!(home.run(Case::new(pair()))?.plan.emitted.len(), 1);
    assert!(skill_path(&home, DEPLOY).exists());
    if let Some(entry) = home.ledger.get_mut(&key(DEPLOY)) {
        entry.first_emitted = (today() - chrono::Duration::days(31)).to_string();
    }
    let retired = home.run(Case::new(pair()))?;
    assert_eq!(retired.settled.retired, vec![key(DEPLOY)]);
    assert!(!skill_path(&home, DEPLOY).exists());
    assert!(
        !home
            .skills()
            .join(DEPLOY)
            .join("references/lessons.md")
            .exists()
    );
    assert!(home.ledger[&key(DEPLOY)].retired_at.is_some());
    let held = home.run(Case::new(pair()))?;
    assert_eq!(held.plan.held_retired, vec![key(DEPLOY)]);
    assert!(!skill_path(&home, DEPLOY).exists());
    let regained = home.run(Case::new(lessons(DEPLOY, 1..4)))?;
    assert!(regained.plan.held_retired.is_empty());
    assert!(skill_path(&home, DEPLOY).exists());
    assert!(home.ledger[&key(DEPLOY)].retired_at.is_none());
    Ok(())
}

#[test]
fn a_skill_with_ten_loads_and_a_corrected_rate_above_base_is_retired() -> anyhow::Result<()> {
    let mut home = Home::new()?;
    let mut first = Case::new(pair());
    first.loads = loaded(DEPLOY);
    home.run(first)?;
    assert!(skill_path(&home, DEPLOY).exists());
    let mut second = Case::new(pair());
    second.loads = loaded(DEPLOY);
    second.measured = measured(DEPLOY, 12, 8, 4);
    let outcome = home.run(second)?;
    assert_eq!(outcome.settled.harmful, vec![key(DEPLOY)]);
    assert!(outcome.settled.retired.is_empty());
    assert!(!skill_path(&home, DEPLOY).exists());
    assert!(
        !home
            .skills()
            .join(DEPLOY)
            .join("references/lessons.md")
            .exists()
    );
    let entry = &home.ledger[&key(DEPLOY)];
    assert_eq!(
        entry.retired_reason.as_deref(),
        Some("corrected_above_base")
    );
    assert!(!entry.published);
    let recorded = entry.outcome.clone().unwrap_or_default();
    assert_eq!(recorded["retired_reason"], "corrected_above_base");
    assert_eq!(recorded["loads"], 12);
    assert_eq!(recorded["corrected_turns"], 4);
    assert_eq!(recorded["first_shipped_on"], today().to_string());
    let held = home.run(Case::new(pair()))?;
    assert_eq!(held.plan.held_retired, vec![key(DEPLOY)]);
    Ok(())
}

#[test]
fn a_skill_with_a_rate_at_or_below_base_is_kept_and_counted_as_live() -> anyhow::Result<()> {
    let mut home = Home::new()?;
    let mut first = Case::new(pair());
    first.loads = loaded(DEPLOY);
    home.run(first)?;
    let mut second = Case::new(pair());
    second.loads = loaded(DEPLOY);
    second.measured = measured(DEPLOY, 12, 8, 1);
    let outcome = home.run(second)?;
    assert!(outcome.settled.harmful.is_empty());
    assert!(skill_path(&home, DEPLOY).exists());
    assert!(home.ledger[&key(DEPLOY)].outcome.is_some());
    Ok(())
}
