use super::answer::{MissingAnswer, Outdated, Tighten, Validated};
use super::decide::{EvidenceItem, Stmt};
use super::rewrite::{RewriteKind, plan_rewrites};
use super::test_support::{harness, judge, vector};
use super::undo::undo_decision;
use chrono::{Duration, Utc};
use serde_json::json;
use std::collections::{HashMap, HashSet};

fn lesson(id: i64, text: &str, days_ago: i64) -> Stmt {
    Stmt {
        id,
        item_id: id + 100,
        kind: "lesson".to_owned(),
        source_name: None,
        carrier: format!("lesson:{id}"),
        text: text.to_owned(),
        passage: None,
        passage_start: None,
        passage_end: None,
        stated_at: Utc::now() - Duration::days(days_ago),
        lesson_id: Some(id),
        lesson_active: true,
        current: true,
        project_id: Some(1),
        machine_wide: false,
        relative_path: None,
        always_loaded: false,
    }
}

fn proof(kind: &str, days_ago: i64) -> EvidenceItem {
    EvidenceItem {
        id: 1_000_000,
        reference: "correction:1".to_owned(),
        kind: kind.to_owned(),
        dated_at: Utc::now() - Duration::days(days_ago),
    }
}

fn validated(outdated: Vec<Outdated>, tighten: Vec<Tighten>) -> Validated {
    Validated { outdated, tighten, ..Validated::default() }
}

fn fix(id: i64) -> Outdated {
    Outdated { id, evidence_id: 1_000_000, text: "Use jj.".to_owned() }
}

#[test]
fn a_correction_rewrites_only_a_statement_older_than_the_operators_evidence() {
    let stmts: HashMap<i64, Stmt> = [lesson(1, "Use git.", 5)].into_iter().map(|s| (s.id, s)).collect();
    for kind in ["correction", "unmet_ask", "question_answer"] {
        let out = plan_rewrites(&validated(vec![fix(1)], vec![]), &stmts, &[proof(kind, 1)], &HashSet::new());
        assert_eq!(out.len(), 1, "{kind}");
        assert_eq!(out[0].kind, RewriteKind::Corrected);
    }
    for bad in [proof("note", 1), proof("correction", 9)] {
        assert!(plan_rewrites(&validated(vec![fix(1)], vec![]), &stmts, &[bad], &HashSet::new()).is_empty());
    }
    let retired: HashSet<i64> = [1].into_iter().collect();
    assert!(plan_rewrites(&validated(vec![fix(1)], vec![]), &stmts, &[proof("correction", 1)], &retired).is_empty());
}

#[test]
fn tightening_needs_two_hundred_characters_and_at_most_three_quarters_of_them() {
    let long = "x".repeat(200);
    let stmts: HashMap<i64, Stmt> = [lesson(1, &long, 5), lesson(2, "short text", 5)].into_iter().map(|s| (s.id, s)).collect();
    let ok = Tighten { id: 1, text: "y".repeat(150) };
    let too_long = Tighten { id: 1, text: "y".repeat(151) };
    let short_original = Tighten { id: 2, text: "s".to_owned() };
    assert_eq!(plan_rewrites(&validated(vec![], vec![ok]), &stmts, &[], &HashSet::new()).len(), 1);
    assert!(plan_rewrites(&validated(vec![], vec![too_long]), &stmts, &[], &HashSet::new()).is_empty());
    assert!(plan_rewrites(&validated(vec![], vec![short_original]), &stmts, &[], &HashSet::new()).is_empty());
    let both = validated(vec![fix(1)], vec![Tighten { id: 1, text: "y".repeat(100) }]);
    let out = plan_rewrites(&both, &stmts, &[proof("correction", 1)], &HashSet::new());
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].kind, RewriteKind::Corrected);
}

#[tokio::test]
async fn an_operator_correction_rewrites_the_lesson_and_undo_puts_the_old_text_back() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    h.emb.set("Use git.", &[(5, 1.0)]);
    let project = h.fx.project().await?;
    let lesson = h.fx.lesson(project, "Use git.").await?;
    h.fx.evidence("correction", project, "No, use jj.", &vector(&[(5, 1.0)])).await?;
    h.items().await?;
    let sid = h.statement_id(&lesson).await?;
    h.llm.push_judge(Ok(judge(json!({
        "subject": "VCS", "text": "Use jj.",
        "members": [{"id": sid, "role": "keep", "by": null, "note": "", "evidence_for": null}],
        "pull_in": [], "evidence_used": [], "merges": [],
        "outdated": [{"id": sid, "evidence_id": 1_000_000, "text": "Use jj."}], "tighten": []
    }))?));
    h.subjects(None).await?;
    assert_eq!(h.fx.retirement(lesson.source_id).await?.2, "Use jj.");
    let decisions = h.decisions().await?;
    assert_eq!(decisions.len(), 1);
    assert_eq!((decisions[0].verdict.as_str(), decisions[0].application.as_str()), ("corrected", "applied"));
    assert_eq!(decisions[0].text_before, "Use git.");
    assert_eq!(decisions[0].text_after.as_deref(), Some("Use jj."));

    undo_decision(&h.fx.handle, decisions[0].id).await.map_err(|e| anyhow::anyhow!("{e:?}"))?;
    assert_eq!(h.fx.retirement(lesson.source_id).await?.2, "Use git.");
    h.fx.db.cleanup().await?;
    Ok(())
}

const WORDY: &str = "When you are about to make a commit in this repository, you should always take care to first run the complete test suite and then the linter, and only after both of them have passed cleanly should you go ahead and create the commit itself.";

async fn wordy(h: &mut super::test_support::Harness) -> anyhow::Result<(super::test_support::Carrier, i64, i64)> {
    h.emb.set(WORDY, &[(5, 1.0)]);
    h.emb.set("Commit in small steps.", &[(5, 0.99), (6, 0.1)]);
    let project = h.fx.project().await?;
    let long = h.fx.lesson(project, WORDY).await?;
    let other = h.fx.lesson(project, "Commit in small steps.").await?;
    h.items().await?;
    let (a, b) = (h.statement_id(&long).await?, h.statement_id(&other).await?);
    Ok((long, a, b))
}

fn tighten_answer(a: i64, b: i64) -> anyhow::Result<super::answer::JudgeAnswer> {
    judge(json!({
        "subject": "Commits", "text": "",
        "members": [
            {"id": a, "role": "keep", "by": null, "note": "", "evidence_for": null},
            {"id": b, "role": "keep", "by": null, "note": "", "evidence_for": null}],
        "pull_in": [], "evidence_used": [], "merges": [], "outdated": [],
        "tighten": [{"id": a, "text": "Run the tests and the linter before every commit."}]
    }))
}

#[tokio::test]
async fn a_wordy_lesson_is_tightened_only_when_the_check_finds_nothing_lost() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let (long, a, b) = wordy(&mut h).await?;
    h.llm.push_judge(Ok(tighten_answer(a, b)?));
    h.llm.push_check(Ok(MissingAnswer { missing: vec!["run the linter".to_owned()] }));
    h.subjects(None).await?;
    assert_eq!(h.fx.retirement(long.source_id).await?.2, WORDY);
    assert!(h.decisions().await?.iter().all(|d| d.verdict != "tightened"));

    h.fx.make_subjects_due().await?;
    sqlx::query("UPDATE memory.knowledge_subjects SET changed_at = now(), judge_after = now() - interval '1 second'")
        .execute(h.fx.handle.pool())
        .await?;
    h.llm.push_judge(Ok(tighten_answer(a, b)?));
    h.llm.push_check(Ok(MissingAnswer { missing: vec![] }));
    h.subjects(None).await?;
    assert_eq!(h.fx.retirement(long.source_id).await?.2, "Run the tests and the linter before every commit.");
    let d = h.decisions().await?;
    let t = d.iter().find(|d| d.verdict == "tightened").ok_or_else(|| anyhow::anyhow!("no tightened"))?;
    assert_eq!(t.text_before, WORDY);
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_correction_rewrites_the_passage_in_the_file_and_undo_reinserts_it() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    h.settings.edit_files = true;
    let body = "- Keep this one.\n- Commits use jj.\n";
    h.emb.set("Commits use jj.", &[(5, 1.0)]);
    let project = h.fx.project().await?;
    std::fs::write(h.root.path().join("git.md"), body)?;
    let file = h.fx.checkout_file(project, "rules", "git.md", body, h.root.path()).await?;
    h.fx.evidence("correction", project, "Also sign commits.", &vector(&[(5, 1.0)])).await?;
    h.llm.push_split(0, Ok(super::test_support::split(&[("Commits use jj.", "Commits use jj.")])));
    h.items().await?;
    let sid = h.statement_id(&file.carrier).await?;
    h.llm.push_judge(Ok(judge(json!({
        "subject": "Commits", "text": "",
        "members": [{"id": sid, "role": "keep", "by": null, "note": "", "evidence_for": null}],
        "pull_in": [], "evidence_used": [], "merges": [], "tighten": [],
        "outdated": [{"id": sid, "evidence_id": 1_000_000, "text": "Commits use jj and are signed."}]
    }))?));
    h.subjects(Some(file.machine)).await?;
    assert_eq!(std::fs::read_to_string(h.root.path().join("git.md"))?, "- Keep this one.\n- Commits use jj and are signed.\n");
    let d = h.decisions().await?;
    let c = d.iter().find(|d| d.verdict == "corrected").ok_or_else(|| anyhow::anyhow!("no corrected"))?;
    assert_eq!((c.application.as_str(), c.text_before.as_str()), ("applied", "Commits use jj."));
    assert_eq!(c.text_after.as_deref(), Some("Commits use jj and are signed."));

    undo_decision(&h.fx.handle, c.id).await.map_err(|e| anyhow::anyhow!("{e:?}"))?;
    assert_eq!(std::fs::read_to_string(h.root.path().join("git.md"))?, body);
    h.fx.db.cleanup().await?;
    Ok(())
}
