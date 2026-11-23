use super::settle::settle_conflict;
use super::test_support::{Carrier, Harness, harness, judge, split};
use super::undo::DecisionError;
use serde_json::json;

async fn open_lesson_conflict(h: &mut Harness) -> anyhow::Result<(Carrier, Carrier, i64, i64, i64)> {
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
    let id = h.decisions().await?.iter().find(|d| d.verdict == "conflict_open").map(|d| d.id)
        .ok_or_else(|| anyhow::anyhow!("no open conflict"))?;
    Ok((a, b, sa, sb, id))
}

#[tokio::test]
async fn keeping_one_side_supersedes_the_other_lesson_and_records_the_operators_choice() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let (a, b, sa, sb, id) = open_lesson_conflict(&mut h).await?;
    let settled = settle_conflict(&h.fx.handle, None, id, sb).await.map_err(|e| anyhow::anyhow!("{e:?}"))?;
    assert_eq!(settled.decision.verdict, "superseded");
    assert_eq!(settled.decision.application, "applied");
    assert_eq!(settled.decision.model, "operator");
    assert_eq!(settled.decision.statement_id, Some(sa));
    assert_eq!(settled.decision.counterpart_statement_id, Some(sb));
    assert_eq!(h.fx.retirement(a.source_id).await?.0.as_deref(), Some("consolidated"));
    assert_eq!(h.fx.retirement(b.source_id).await?.0, None);
    assert_eq!(h.fx.statements_of(a.item_id).await?[0].2, "superseded");

    let again = settle_conflict(&h.fx.handle, None, id, sb).await;
    assert!(matches!(again, Err(DecisionError::Conflict(_))));
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn settling_refuses_a_statement_outside_the_pair_and_a_decision_that_is_not_an_open_conflict() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let (_, _, _, _, id) = open_lesson_conflict(&mut h).await?;
    assert!(matches!(settle_conflict(&h.fx.handle, None, id, i64::MAX).await, Err(DecisionError::Conflict(_))));
    assert!(matches!(settle_conflict(&h.fx.handle, None, i64::MAX, 1).await, Err(DecisionError::NotFound)));
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_file_that_loses_is_proposed_while_edit_files_is_off_and_removed_when_it_is_on() -> anyhow::Result<()> {
    for edit in [false, true] {
        let Some(mut h) = harness(1).await? else {
            return Ok(());
        };
        h.settings.edit_files = edit;
        sqlx::query("INSERT INTO config.settings(key, value) VALUES ('knowledge_consolidation.edit_files', to_jsonb($1::boolean))")
            .bind(edit)
            .execute(h.fx.handle.pool())
            .await?;
        h.emb.set("Use git for commits.", &[(5, 1.0)]);
        h.emb.set("Use jj for commits.", &[(5, 0.99), (6, 0.1)]);
        let project = h.fx.project().await?;
        let body_a = "- Use git for commits.\n- Other A.\n";
        let body_b = "- Use jj for commits.\n- Other B.\n";
        std::fs::write(h.root.path().join("a.md"), body_a)?;
        let fa = h.fx.checkout_file(project, "rules", "a.md", body_a, h.root.path()).await?;
        let other = tempfile::tempdir()?;
        std::fs::write(other.path().join("b.md"), body_b)?;
        let fb = h.fx.checkout_file(project, "rules", "b.md", body_b, other.path()).await?;
        h.llm.push_split(0, Ok(split(&[("Use git for commits.", "Use git for commits.")])));
        h.llm.push_split(0, Ok(split(&[("Use jj for commits.", "Use jj for commits.")])));
        h.items().await?;
        let (sa, sb) = (h.statement_id(&fa.carrier).await?, h.statement_id(&fb.carrier).await?);
        h.llm.push_judge(Ok(judge(json!({
            "subject": "VCS", "text": "",
            "members": [
                {"id": sa, "role": "conflicts", "by": sb, "note": "", "evidence_for": null},
                {"id": sb, "role": "conflicts", "by": sa, "note": "", "evidence_for": null}],
            "pull_in": [], "evidence_used": [], "merges": []
        }))?));
        h.subjects(Some(fa.machine)).await?;
        let Some(conflict) = h.decisions().await?.iter().find(|d| d.verdict == "conflict_open").map(|d| d.id) else {
            anyhow::bail!("no open conflict");
        };
        let loser_is_a = h.fx.statements_of(fa.carrier.item_id).await?[0].2 == "current";
        assert!(loser_is_a);
        let settled = settle_conflict(&h.fx.handle, Some(fa.machine), conflict, sb).await.map_err(|e| anyhow::anyhow!("{e:?}"))?;
        assert_eq!(settled.decision.text_after.as_deref(), Some(""));
        let disk = std::fs::read_to_string(h.root.path().join("a.md"))?;
        if edit {
            assert_eq!(settled.decision.application, "applied");
            assert_eq!(disk, "- Other A.\n");
        } else {
            assert_eq!(settled.decision.application, "proposed");
            assert_eq!(disk, body_a);
        }
        h.fx.db.cleanup().await?;
    }
    Ok(())
}
