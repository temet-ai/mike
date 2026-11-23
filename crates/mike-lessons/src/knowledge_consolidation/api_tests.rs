use crate::api::knowledge_consolidation_dto::{decisions_json, statements_json};
use super::test_support::{covered_answer, covered_lesson_scenario, harness, judge};
use crate::store::persistence::consolidation::DecisionFilter;
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use serde_json::json;

const BODY: &str = "- Keep this one.\n- Commits use jj.\n";

#[tokio::test]
async fn decisions_carry_readable_carriers_the_counterpart_text_and_a_fixed_evidence_shape() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let s = covered_lesson_scenario(&mut h, BODY, "Commits use jj.").await?;
    h.llm.push_judge(Ok(covered_answer(s.lesson_statement, s.file_statement)?));
    h.subjects(None).await?;
    let project = sqlx::query_scalar::<_, i64>("SELECT project_id FROM memory.lessons WHERE id = $1")
        .bind(s.lesson.source_id)
        .fetch_one(h.fx.handle.pool())
        .await?;
    let rows = h.fx.handle.knowledge().list_consolidation_decisions(project, None, None, 10).await?;
    let json = decisions_json(&h.fx.handle, &rows).await.map_err(|e| anyhow::anyhow!(e))?;
    let covered = json
        .iter()
        .find(|d| d["verdict"] == "covered")
        .ok_or_else(|| anyhow::anyhow!("no covered decision"))?;
    assert_eq!(covered["carrier"]["kind"], "lesson");
    assert_eq!(covered["carrier"]["label"], format!("Lesson {}", s.lesson.source_id));
    assert_eq!(covered["carrier"]["lesson_id"], s.lesson.source_id);
    assert_eq!(covered["counterpart"]["kind"], "file");
    assert_eq!(covered["counterpart"]["label"], "rules · git.md");
    assert_eq!(covered["counterpart"]["relative_path"], "git.md");
    assert_eq!(covered["counterpart"]["scope"], "project");
    assert_eq!(covered["counterpart_text"], "Commits use jj.");
    assert!(covered["evidence"].is_array());
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn the_verdict_and_open_filters_return_only_unsettled_conflicts_and_the_named_verdicts() -> anyhow::Result<()> {
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
            {"id": sa, "role": "conflicts", "by": sb, "note": "", "evidence_for": null},
            {"id": sb, "role": "conflicts", "by": sa, "note": "", "evidence_for": null}],
        "pull_in": [], "evidence_used": [], "merges": []
    }))?));
    h.subjects(None).await?;
    let knowledge = h.fx.handle.knowledge();
    let only_open = ["conflict_open".to_owned()];
    let open = knowledge
        .list_consolidation_decisions_filtered(project, None, None, 10, DecisionFilter { verdicts: Some(&only_open), open_only: true })
        .await?;
    assert_eq!(open.len(), 1);
    let kept_only = ["kept".to_owned()];
    let none = knowledge
        .list_consolidation_decisions_filtered(project, None, None, 10, DecisionFilter { verdicts: Some(&kept_only), open_only: false })
        .await?;
    assert!(none.is_empty());

    sqlx::query("UPDATE memory.knowledge_statements SET state = 'superseded' WHERE id = $1")
        .bind(sa)
        .execute(h.fx.handle.pool())
        .await?;
    let settled = knowledge
        .list_consolidation_decisions_filtered(project, None, None, 10, DecisionFilter { verdicts: Some(&only_open), open_only: true })
        .await?;
    assert!(settled.is_empty());
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_subject_lists_its_statements_in_every_state_with_carriers() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let s = covered_lesson_scenario(&mut h, BODY, "Commits use jj.").await?;
    h.llm.push_judge(Ok(covered_answer(s.lesson_statement, s.file_statement)?));
    h.subjects(None).await?;
    let subject = h.fx.statements_of(s.lesson.item_id).await?[0].3.ok_or_else(|| anyhow::anyhow!("no subject"))?;
    let knowledge = h.fx.handle.knowledge();
    let summary = knowledge.get_knowledge_subject(subject).await?.ok_or_else(|| anyhow::anyhow!("no summary"))?;
    assert_eq!(summary.id, subject);
    let ids = knowledge.subject_statement_ids_all(subject).await?;
    assert_eq!(ids.len(), 2);
    let statements = statements_json(&h.fx.handle, &ids).await.map_err(|e| anyhow::anyhow!(e))?;
    let states: Vec<&str> = statements.iter().filter_map(|s| s["state"].as_str()).collect();
    assert!(states.contains(&"covered") && states.contains(&"current"));
    let covered = statements.iter().find(|s| s["state"] == "covered").ok_or_else(|| anyhow::anyhow!("no covered statement"))?;
    assert_eq!(covered["resolved_by_statement_id"], s.file_statement);
    assert_eq!(covered["carrier"]["lesson_id"], s.lesson.source_id);
    assert!(knowledge.get_knowledge_subject(i64::MAX).await?.is_none());
    h.fx.db.cleanup().await?;
    Ok(())
}
