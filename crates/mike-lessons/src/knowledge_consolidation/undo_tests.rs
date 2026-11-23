use super::subjects_tests::{merge_answer, two_lessons};
use super::answer::MissingAnswer;
use super::test_support::{covered_answer, covered_lesson_scenario, harness};
use super::undo::{DecisionError, apply_decision, apply_proposed_on_machine, undo_decision};

const BODY: &str = "- Keep this one.\n- Commits use jj.\n";

async fn decision_id(h: &super::test_support::Harness, verdict: &str) -> anyhow::Result<i64> {
    h.decisions()
        .await?
        .iter()
        .find(|d| d.verdict == verdict)
        .map(|d| d.id)
        .ok_or_else(|| anyhow::anyhow!("no {verdict} decision"))
}

#[tokio::test]
async fn undoing_a_covered_lesson_restores_it_and_its_statement_and_cannot_be_repeated() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let s = covered_lesson_scenario(&mut h, BODY, "Commits use jj.").await?;
    h.llm.push_judge(Ok(covered_answer(s.lesson_statement, s.file_statement)?));
    h.subjects(None).await?;
    let id = decision_id(&h, "covered").await?;
    assert_eq!(h.fx.retirement(s.lesson.source_id).await?.0.as_deref(), Some("consolidated"));

    let undone = undo_decision(&h.fx.handle, id).await;
    assert_eq!(undone.map(|d| d.application), Ok("undone".to_owned()));
    assert_eq!(h.fx.retirement(s.lesson.source_id).await?.0, None);
    assert_eq!(h.fx.statements_of(s.lesson.item_id).await?[0].2, "current");

    assert!(matches!(undo_decision(&h.fx.handle, id).await, Err(DecisionError::Conflict(_))));
    assert_eq!(undo_decision(&h.fx.handle, i64::MAX).await, Err(DecisionError::NotFound));
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn undoing_a_merge_puts_the_earlier_text_back_and_restores_the_retired_lesson() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let (a, b, sa, sb) = two_lessons(&mut h).await?;
    h.llm.push_judge(Ok(merge_answer(sa, sb)?));
    h.llm.push_check(Ok(MissingAnswer { missing: vec![] }));
    h.subjects(None).await?;
    assert_ne!(h.fx.retirement(a.source_id).await?.2, "Run clippy.");

    undo_decision(&h.fx.handle, decision_id(&h, "tightened").await?).await.map_err(|e| anyhow::anyhow!("{e:?}"))?;
    undo_decision(&h.fx.handle, decision_id(&h, "merged").await?).await.map_err(|e| anyhow::anyhow!("{e:?}"))?;
    assert_eq!(h.fx.retirement(a.source_id).await?, (None, None, "Run clippy.".to_owned()));
    assert_eq!(h.fx.retirement(b.source_id).await?.0, None);
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_proposed_file_edit_is_applied_on_demand_and_a_file_edit_is_undone_by_reinsertion() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let s = super::test_support::covered_file_scenario(&mut h, "rules").await?;
    h.llm.push_judge(Ok(covered_answer(s.dup, s.keep)?));
    h.subjects(Some(s.file.machine)).await?;
    let id = decision_id(&h, "covered").await?;
    assert_eq!(std::fs::read_to_string(h.root.path().join("git.md"))?, super::test_support::FILE_BODY);

    let applied = apply_decision(&h.fx.handle, Some(s.file.machine), id).await;
    assert_eq!(applied.map(|d| d.application), Ok("applied".to_owned()));
    assert_eq!(std::fs::read_to_string(h.root.path().join("git.md"))?, super::test_support::FILE_AFTER);
    assert!(matches!(apply_decision(&h.fx.handle, Some(s.file.machine), id).await, Err(DecisionError::Conflict(_))));

    let undone = undo_decision(&h.fx.handle, id).await;
    assert_eq!(undone.map(|d| d.application), Ok("undone".to_owned()));
    assert_eq!(std::fs::read_to_string(h.root.path().join("git.md"))?, super::test_support::FILE_BODY);
    assert_eq!(h.fx.statements_of(s.file.carrier.item_id).await?[0].2, "current");
    assert!(matches!(apply_decision(&h.fx.handle, Some(s.file.machine), id).await, Err(DecisionError::Conflict(_))));
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_file_edit_is_not_undone_when_the_file_changed_around_the_removed_text() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    h.settings.edit_files = true;
    let s = super::test_support::covered_file_scenario(&mut h, "rules").await?;
    h.llm.push_judge(Ok(covered_answer(s.dup, s.keep)?));
    h.subjects(Some(s.file.machine)).await?;
    let id = decision_id(&h, "covered").await?;
    std::fs::write(h.root.path().join("git.md"), "- Totally different now.\n")?;
    assert!(matches!(undo_decision(&h.fx.handle, id).await, Err(DecisionError::Conflict(_))));
    assert_eq!(std::fs::read_to_string(h.root.path().join("git.md"))?, "- Totally different now.\n");
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_failed_file_decision_can_be_retried_once_the_file_matches_again() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    h.settings.edit_files = true;
    let s = super::test_support::covered_file_scenario(&mut h, "rules").await?;
    std::fs::write(h.root.path().join("git.md"), "- Rewritten.\n")?;
    h.llm.push_judge(Ok(covered_answer(s.dup, s.keep)?));
    h.subjects(Some(s.file.machine)).await?;
    let id = decision_id(&h, "covered").await?;
    assert_eq!(h.decisions().await?.iter().find(|d| d.id == id).map(|d| d.application.clone()), Some("failed".to_owned()));

    std::fs::write(h.root.path().join("git.md"), super::test_support::FILE_BODY)?;
    let retried = apply_decision(&h.fx.handle, Some(s.file.machine), id).await;
    assert_eq!(retried.map(|d| d.application), Ok("applied".to_owned()));
    assert_eq!(std::fs::read_to_string(h.root.path().join("git.md"))?, super::test_support::FILE_AFTER);
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn the_lease_free_pass_skips_checkout_files_and_other_machines() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let s = super::test_support::covered_file_scenario(&mut h, "rules").await?;
    h.llm.push_judge(Ok(covered_answer(s.dup, s.keep)?));
    h.subjects(Some(s.file.machine)).await?;
    let id = decision_id(&h, "covered").await?;
    assert_eq!(h.decisions().await?.iter().find(|d| d.id == id).map(|d| d.application.clone()), Some("proposed".to_owned()));

    assert_eq!(apply_proposed_on_machine(&h.fx.handle, uuid::Uuid::new_v4(), 10).await, 0);
    assert_eq!(std::fs::read_to_string(h.root.path().join("git.md"))?, super::test_support::FILE_BODY);
    assert_eq!(apply_proposed_on_machine(&h.fx.handle, s.file.machine, 10).await, 0);
    assert_eq!(std::fs::read_to_string(h.root.path().join("git.md"))?, super::test_support::FILE_BODY);
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn applying_refuses_a_changed_file_and_a_machine_without_the_file() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let s = super::test_support::covered_file_scenario(&mut h, "rules").await?;
    h.llm.push_judge(Ok(covered_answer(s.dup, s.keep)?));
    h.subjects(Some(s.file.machine)).await?;
    let id = decision_id(&h, "covered").await?;

    let elsewhere = apply_decision(&h.fx.handle, Some(uuid::Uuid::new_v4()), id).await;
    assert!(matches!(elsewhere, Err(DecisionError::Conflict(_))));
    std::fs::write(h.root.path().join("git.md"), "- Rewritten.\n")?;
    let changed = apply_decision(&h.fx.handle, Some(s.file.machine), id).await;
    assert!(matches!(changed, Err(DecisionError::Conflict(ref m)) if m.contains("not touched")));
    assert_eq!(std::fs::read_to_string(h.root.path().join("git.md"))?, "- Rewritten.\n");
    assert_eq!(apply_decision(&h.fx.handle, None, i64::MAX).await, Err(DecisionError::NotFound));
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn the_lease_free_pass_applies_machine_owned_files_on_its_own_machine_only() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let s = super::test_support::covered_file_scenario(&mut h, "claude-memory").await?;
    h.llm.push_judge(Ok(covered_answer(s.dup, s.keep)?));
    h.subjects(Some(s.file.machine)).await?;
    assert_eq!(apply_proposed_on_machine(&h.fx.handle, uuid::Uuid::new_v4(), 10).await, 0);
    assert_eq!(apply_proposed_on_machine(&h.fx.handle, s.file.machine, 10).await, 1);
    assert_eq!(std::fs::read_to_string(h.root.path().join("git.md"))?, super::test_support::FILE_AFTER);
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_proposed_decision_whose_statement_is_gone_is_closed_with_the_reason_in_its_text() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let s = super::test_support::covered_file_scenario(&mut h, "rules").await?;
    h.llm.push_judge(Ok(covered_answer(s.dup, s.keep)?));
    h.subjects(Some(s.file.machine)).await?;
    sqlx::query("UPDATE memory.knowledge_statements SET state = 'gone', resolved_by_statement_id = NULL WHERE id = $1")
        .bind(s.dup)
        .execute(h.fx.handle.pool())
        .await?;
    apply_proposed_on_machine(&h.fx.handle, s.file.machine, 10).await;
    let d = h.decisions().await?;
    let closed = d.iter().find(|d| d.verdict == "covered").ok_or_else(|| anyhow::anyhow!("no covered decision"))?;
    assert_eq!(closed.application, "none");
    assert!(closed.reason.contains("the file changed before the edit was made"));
    assert_eq!(std::fs::read_to_string(h.root.path().join("git.md"))?, super::test_support::FILE_BODY);
    h.fx.db.cleanup().await?;
    Ok(())
}
