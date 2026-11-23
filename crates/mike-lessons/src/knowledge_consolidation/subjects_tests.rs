use super::test_support::{covered_answer, covered_lesson_scenario, harness, judge};
use super::llm::Failure;
use super::answer::MissingAnswer;
use serde_json::json;

const BODY: &str = "- Keep this one.\n- Commits use jj.\n";

#[tokio::test]
async fn a_lesson_covered_by_a_file_statement_is_retired_and_the_decision_recorded() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let s = covered_lesson_scenario(&mut h, BODY, "Commits use jj.").await?;
    h.llm.push_judge(Ok(covered_answer(s.lesson_statement, s.file_statement)?));

    let (tally, run_id) = h.subjects(None).await?;
    assert_eq!((tally.subjects_judged, tally.decisions, tally.applied), (1, 2, 1));
    assert_eq!(h.llm.judged(), 1);
    let (kind, by, _) = h.fx.retirement(s.lesson.source_id).await?;
    assert_eq!(kind.as_deref(), Some("consolidated"));
    assert_eq!(by, None);

    let decisions = h.decisions().await?;
    let covered = decisions
        .iter()
        .find(|d| d.verdict == "covered")
        .ok_or_else(|| anyhow::anyhow!("no covered decision"))?;
    assert_eq!(covered.application, "applied");
    assert_eq!(covered.statement_id, Some(s.lesson_statement));
    assert_eq!(covered.counterpart_statement_id, Some(s.file_statement));
    assert_eq!(covered.text_before, "Use jj.");
    assert!(covered.reason.contains("Covered by") && covered.reason.contains("says the same"));
    assert!(decisions.iter().any(|d| d.verdict == "kept" && d.application == "none"));

    let rows = h.fx.statements_of(s.lesson.item_id).await?;
    assert_eq!(rows[0].2, "covered");
    let (title, text, changed, judged_by_run): (String, String, bool, i64) = sqlx::query_as(
        "SELECT k.title, k.text, k.changed_at IS NULL,
                (SELECT count(*) FROM memory.knowledge_consolidation_decisions d WHERE d.run_id = $1)
           FROM memory.knowledge_subjects k WHERE k.id = $2",
    )
    .bind(run_id)
    .bind(rows[0].3)
    .fetch_one(h.fx.handle.pool())
    .await?;
    assert_eq!((title.as_str(), text.as_str(), changed, judged_by_run), ("Commits use jj", "Use jj for commits.", true, 2));
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_subject_with_one_member_and_nothing_near_never_reaches_the_model() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let project = h.fx.project().await?;
    h.fx.lesson(project, "Use jj.").await?;
    h.items().await?;
    let (tally, _) = h.subjects(None).await?;
    assert_eq!((tally.subjects_skipped, tally.subjects_judged, tally.decisions), (1, 0, 0));
    assert_eq!(h.llm.judged(), 0);
    assert_eq!(h.backlog().await?, 0);
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn the_daily_budget_stops_judging_and_leaves_subjects_waiting() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    h.settings.max_judgements_per_day = 1;
    let s = covered_lesson_scenario(&mut h, BODY, "Commits use jj.").await?;
    h.llm.push_judge(Ok(covered_answer(s.lesson_statement, s.file_statement)?));
    h.subjects(None).await?;
    assert_eq!(h.llm.judged(), 1);

    h.emb.set("Run clippy.", &[(30, 1.0)]);
    h.emb.set("Always run clippy.", &[(30, 0.99), (31, 0.1)]);
    let project = h.fx.project().await?;
    h.fx.lesson(project, "Run clippy.").await?;
    h.fx.lesson(project, "Always run clippy.").await?;
    h.items().await?;
    let (tally, _) = h.subjects(None).await?;
    assert_eq!(tally.subjects_judged + tally.subjects_skipped, 0);
    assert_eq!(h.llm.judged(), 1);
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_failed_judgement_defers_the_subject_and_a_permanent_one_waits_hours() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let _ = covered_lesson_scenario(&mut h, BODY, "Commits use jj.").await?;
    h.llm.push_judge(Err(Failure::transient("rate limited")));
    let (tally, _) = h.subjects(None).await?;
    assert_eq!(tally.subjects_judged, 0);
    assert!(tally.last_error.as_deref().unwrap_or_default().contains("rate limited"));
    let (waiting, later): (bool, bool) = sqlx::query_as(
        "SELECT changed_at IS NOT NULL, judge_after > now() + interval '30 seconds' FROM memory.knowledge_subjects
          WHERE members > 1",
    )
    .fetch_one(h.fx.handle.pool())
    .await?;
    assert!(waiting && later);

    h.llm.push_judge(Err(Failure::permanent("schema")));
    h.subjects(None).await?;
    let hours: bool = sqlx::query_scalar(
        "SELECT judge_after > now() + interval '5 hours' FROM memory.knowledge_subjects WHERE members > 1",
    )
    .fetch_one(h.fx.handle.pool())
    .await?;
    assert!(hours);
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_file_statement_beats_a_lesson_it_contradicts_on_the_same_day() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let s = covered_lesson_scenario(&mut h, BODY, "Commits use jj.").await?;
    h.llm.push_judge(Ok(judge(json!({
        "subject": "Commits", "text": "Use jj.",
        "members": [
            {"id": s.lesson_statement, "role": "conflicts", "by": s.file_statement, "note": "wants git", "evidence_for": null},
            {"id": s.file_statement, "role": "conflicts", "by": s.lesson_statement, "note": "", "evidence_for": null}],
        "pull_in": [], "evidence_used": [], "merges": []
    }))?));
    let (tally, _) = h.subjects(None).await?;
    assert_eq!((tally.decisions, tally.applied), (1, 1));
    let decision = &h.decisions().await?[0];
    assert_eq!((decision.verdict.as_str(), decision.application.as_str()), ("superseded", "applied"));
    assert!(decision.reason.contains("hand-written file wins"));
    assert_eq!(h.fx.retirement(s.lesson.source_id).await?.0.as_deref(), Some("consolidated"));
    assert_eq!(h.fx.statements_of(s.lesson.item_id).await?[0].2, "superseded");
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn two_lessons_that_contradict_on_the_same_day_stay_open_and_nothing_is_retired() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    h.emb.set("Use git.", &[(5, 1.0)]);
    h.emb.set("Use jj.", &[(5, 0.99), (6, 0.1)]);
    let project = h.fx.project().await?;
    let (a, b) = (h.fx.lesson(project, "Use git.").await?, h.fx.lesson(project, "Use jj.").await?);
    h.items().await?;
    let (sa, sb) = (h.statement_id(&a).await?, h.statement_id(&b).await?);
    h.llm.push_judge(Ok(judge(json!({
        "subject": "VCS", "text": "",
        "members": [
            {"id": sa, "role": "conflicts", "by": sb, "note": "git", "evidence_for": null},
            {"id": sb, "role": "conflicts", "by": sa, "note": "jj", "evidence_for": null}],
        "pull_in": [], "evidence_used": [], "merges": []
    }))?));
    h.subjects(None).await?;
    let decisions = h.decisions().await?;
    assert_eq!(decisions.len(), 1);
    assert_eq!((decisions[0].verdict.as_str(), decisions[0].application.as_str()), ("conflict_open", "none"));
    assert_eq!(h.fx.retirement(a.source_id).await?.0, None);
    assert_eq!(h.fx.retirement(b.source_id).await?.0, None);
    h.fx.db.cleanup().await?;
    Ok(())
}

pub(super) async fn two_lessons(h: &mut super::test_support::Harness) -> anyhow::Result<(super::test_support::Carrier, super::test_support::Carrier, i64, i64)> {
    h.emb.set("Run clippy.", &[(5, 1.0)]);
    h.emb.set("Before commit use -D warnings.", &[(5, 0.99), (6, 0.1)]);
    let project = h.fx.project().await?;
    let a = h.fx.lesson(project, "Run clippy.").await?;
    let b = h.fx.lesson(project, "Before commit use -D warnings.").await?;
    h.items().await?;
    let (sa, sb) = (h.statement_id(&a).await?, h.statement_id(&b).await?);
    Ok((a, b, sa, sb))
}

pub(super) fn merge_answer(a: i64, b: i64) -> anyhow::Result<super::answer::JudgeAnswer> {
    judge(json!({
        "subject": "Clippy", "text": "Run clippy -D warnings before commit.",
        "members": [
            {"id": a, "role": "adds", "by": b, "note": "", "evidence_for": null},
            {"id": b, "role": "keep", "by": null, "note": "", "evidence_for": null}],
        "pull_in": [], "evidence_used": [],
        "merges": [{"ids": [a, b], "text": "Run clippy -D warnings before commit."}]
    }))
}

#[tokio::test]
async fn lessons_that_add_to_each_other_are_merged_into_the_oldest_when_the_check_finds_nothing_lost() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let (a, b, sa, sb) = two_lessons(&mut h).await?;
    h.llm.push_judge(Ok(merge_answer(sa, sb)?));
    h.llm.push_check(Ok(MissingAnswer { missing: vec![] }));
    h.subjects(None).await?;

    let (kind_a, _, text_a) = h.fx.retirement(a.source_id).await?;
    assert_eq!((kind_a, text_a.as_str()), (None, "Run clippy -D warnings before commit."));
    let (kind_b, by_b, _) = h.fx.retirement(b.source_id).await?;
    assert_eq!((kind_b.as_deref(), by_b), (Some("consolidated"), Some(a.source_id)));
    let decisions = h.decisions().await?;
    let tightened = decisions.iter().find(|d| d.verdict == "tightened").ok_or_else(|| anyhow::anyhow!("no tightened"))?;
    assert_eq!(tightened.text_before, "Run clippy.");
    assert_eq!(tightened.text_after.as_deref(), Some("Run clippy -D warnings before commit."));
    assert!(decisions.iter().any(|d| d.verdict == "merged" && d.counterpart_statement_id == Some(sa)));
    assert_eq!(h.fx.statements_of(b.item_id).await?[0].2, "merged");
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_merge_is_not_applied_when_the_check_names_a_lost_instruction() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let (a, b, sa, sb) = two_lessons(&mut h).await?;
    h.llm.push_judge(Ok(merge_answer(sa, sb)?));
    h.llm.push_check(Ok(MissingAnswer { missing: vec!["-D warnings".to_owned()] }));
    h.subjects(None).await?;
    assert_eq!(h.fx.retirement(a.source_id).await?, (None, None, "Run clippy.".to_owned()));
    assert_eq!(h.fx.retirement(b.source_id).await?.0, None);
    assert!(h.decisions().await?.iter().all(|d| d.verdict != "tightened" && d.verdict != "merged"));
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_statement_the_judge_says_does_not_belong_leaves_for_another_subject() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    h.emb.set("Use jj.", &[(5, 1.0)]);
    h.emb.set("Use jj, not git.", &[(5, 0.99), (6, 0.1)]);
    let project = h.fx.project().await?;
    let (a, b) = (h.fx.lesson(project, "Use jj.").await?, h.fx.lesson(project, "Use jj, not git.").await?);
    h.items().await?;
    let (sa, sb) = (h.statement_id(&a).await?, h.statement_id(&b).await?);
    let before = h.fx.statements_of(a.item_id).await?[0].3;
    assert_eq!(before, h.fx.statements_of(b.item_id).await?[0].3);
    h.llm.push_judge(Ok(judge(json!({
        "subject": "Jj", "text": "Use jj.",
        "members": [
            {"id": sa, "role": "keep", "by": null, "note": "", "evidence_for": null},
            {"id": sb, "role": "elsewhere", "by": null, "note": "about git", "evidence_for": null}],
        "pull_in": [], "evidence_used": [], "merges": []
    }))?));
    let (tally, _) = h.subjects(None).await?;
    assert_eq!(tally.roles.values().map(|r| r.elsewhere).sum::<u32>(), 1);
    let after_b = h.fx.statements_of(b.item_id).await?[0].3;
    assert!(after_b.is_some() && after_b != before);
    assert_eq!(h.fx.statements_of(a.item_id).await?[0].3, before);
    assert!(h.decisions().await?.iter().any(|d| d.verdict == "misgrouped"));
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_nearby_statement_the_judge_relates_to_a_member_joins_the_subject() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    h.emb.set("Use jj.", &[(5, 1.0)]);
    h.emb.set("Commit with jj and sign.", &[(5, 0.6), (7, 0.8)]);
    let project = h.fx.project().await?;
    let (a, x) = (h.fx.lesson(project, "Use jj.").await?, h.fx.lesson(project, "Commit with jj and sign.").await?);
    h.items().await?;
    let (sa, sx) = (h.statement_id(&a).await?, h.statement_id(&x).await?);
    assert_ne!(h.fx.statements_of(a.item_id).await?[0].3, h.fx.statements_of(x.item_id).await?[0].3);
    h.llm.push_judge(Ok(judge(json!({
        "subject": "Jj", "text": "Use jj and sign commits.",
        "members": [{"id": sa, "role": "keep", "by": null, "note": "", "evidence_for": null}],
        "pull_in": [{"id": sx, "role": "adds", "by": sa, "note": "signing", "evidence_for": null}],
        "evidence_used": [], "merges": []
    }))?));
    h.subjects(None).await?;
    assert_eq!(h.llm.judged(), 1);
    assert_eq!(h.fx.statements_of(a.item_id).await?[0].3, h.fx.statements_of(x.item_id).await?[0].3);
    let decisions = h.decisions().await?;
    assert!(decisions.iter().any(|d| d.statement_id == Some(sx) && d.verdict == "kept" && d.counterpart_statement_id == Some(sa)));
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_covered_file_statement_is_only_proposed_while_edit_files_is_off() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let s = super::test_support::covered_file_scenario(&mut h, "rules").await?;
    h.llm.push_judge(Ok(covered_answer(s.dup, s.keep)?));
    h.subjects(Some(s.file.machine)).await?;
    let d = h.decisions().await?;
    let covered = d.iter().find(|d| d.verdict == "covered").ok_or_else(|| anyhow::anyhow!("no covered decision"))?;
    assert_eq!(covered.application, "proposed");
    assert_eq!(covered.text_before, "Commits use jj.");
    assert_eq!(covered.text_after.as_deref(), Some(""));
    assert_eq!(std::fs::read_to_string(h.root.path().join("git.md"))?, super::test_support::FILE_BODY);
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn with_edit_files_on_the_passage_is_removed_from_the_file_on_this_machine() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    h.settings.edit_files = true;
    let s = super::test_support::covered_file_scenario(&mut h, "rules").await?;
    h.llm.push_judge(Ok(covered_answer(s.dup, s.keep)?));
    h.subjects(Some(s.file.machine)).await?;
    assert_eq!(std::fs::read_to_string(h.root.path().join("git.md"))?, super::test_support::FILE_AFTER);
    let d = h.decisions().await?;
    let covered = d.iter().find(|d| d.verdict == "covered").ok_or_else(|| anyhow::anyhow!("no covered decision"))?;
    assert_eq!(covered.application, "applied");
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_file_whose_bytes_changed_since_the_split_is_left_untouched_and_the_decision_fails() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    h.settings.edit_files = true;
    let s = super::test_support::covered_file_scenario(&mut h, "rules").await?;
    std::fs::write(h.root.path().join("git.md"), "- Someone rewrote this file entirely.\n")?;
    h.llm.push_judge(Ok(covered_answer(s.dup, s.keep)?));
    let (tally, _) = h.subjects(Some(s.file.machine)).await?;
    assert_eq!(tally.failed, 1);
    assert_eq!(std::fs::read_to_string(h.root.path().join("git.md"))?, "- Someone rewrote this file entirely.\n");
    let d = h.decisions().await?;
    let covered = d.iter().find(|d| d.verdict == "covered").ok_or_else(|| anyhow::anyhow!("no covered decision"))?;
    assert_eq!(covered.application, "failed");
    assert!(covered.error.as_deref().unwrap_or_default().contains("not touched"));
    assert_eq!(h.fx.statements_of(s.file.carrier.item_id).await?[0].2, "current");
    assert_eq!(h.backlog().await?, 1);
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_lesson_covered_only_by_an_agent_file_is_kept_with_the_relation_recorded() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let s = super::test_support::covered_scenario_in(&mut h, "agents", BODY, "Commits use jj.").await?;
    h.llm.push_judge(Ok(covered_answer(s.lesson_statement, s.file_statement)?));
    h.subjects(None).await?;
    assert_eq!(h.fx.retirement(s.lesson.source_id).await?.0, None);
    let d = h.decisions().await?;
    let kept = d.iter().find(|d| d.statement_id == Some(s.lesson_statement)).ok_or_else(|| anyhow::anyhow!("no decision"))?;
    assert_eq!((kept.verdict.as_str(), kept.application.as_str()), ("kept", "none"));
    assert_eq!(kept.counterpart_statement_id, Some(s.file_statement));
    assert!(kept.reason.contains("reaches fewer sessions"));
    h.fx.db.cleanup().await?;
    Ok(())
}
