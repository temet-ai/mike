use super::test_fixture::{Fixture, fixture, statement, vector};
use super::{DecisionApplication, NewDecision};
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use serde_json::json;

pub fn decision(run_id: i64, project_id: i64, reference: &str, verdict: &str) -> NewDecision {
    NewDecision {
        run_id,
        project_id: Some(project_id),
        subject_id: None,
        statement_id: None,
        item_id: None,
        carrier_reference: reference.into(),
        verdict: verdict.into(),
        counterpart_statement_id: None,
        counterpart_reference: None,
        text_before: "Before.".into(),
        text_after: None,
        reason: "Because.".into(),
        evidence: json!([{"reference": "correction:1", "similarity": 0.9}]),
        model: "test-judge".into(),
        application: "none".into(),
        applied_at: None,
        error: None,
    }
}

async fn stored(fx: &Fixture, id: i64) -> anyhow::Result<(String, Option<String>, bool)> {
    Ok(sqlx::query_as(
        "SELECT application, error, applied_at IS NOT NULL
           FROM memory.knowledge_consolidation_decisions WHERE id = $1",
    )
    .bind(id)
    .fetch_one(fx.handle.pool())
    .await?)
}

#[tokio::test]
async fn write_decisions_inserts_all_rows_and_returns_ids_in_one_call() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let run = fx.run().await?;
    let ids = fx
        .handle
        .knowledge()
        .write_consolidation_decisions(&[
            decision(run, project, "lesson:a", "kept"),
            NewDecision {
                text_after: Some("After.".into()),
                ..decision(run, project, "lesson:b", "tightened")
            },
        ])
        .await?;
    assert_eq!(ids.len(), 2);
    let rows: Vec<(String, String, Option<String>, serde_json::Value)> = sqlx::query_as(
        "SELECT carrier_reference, verdict, text_after, evidence
           FROM memory.knowledge_consolidation_decisions WHERE run_id = $1 ORDER BY id",
    )
    .bind(run)
    .fetch_all(fx.handle.pool())
    .await?;
    assert_eq!(rows[0].0, "lesson:a");
    assert_eq!(rows[1].1, "tightened");
    assert_eq!(rows[1].2.as_deref(), Some("After."));
    assert_eq!(rows[0].3[0]["reference"], "correction:1");
    assert!(fx.handle.knowledge().write_consolidation_decisions(&[]).await?.is_empty());
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn write_decisions_rejects_unknown_verdict_and_writes_nothing() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let run = fx.run().await?;
    let result = fx
        .handle
        .knowledge()
        .write_consolidation_decisions(&[
            decision(run, project, "lesson:a", "kept"),
            decision(run, project, "lesson:b", "shrugged"),
        ])
        .await;
    assert!(result.is_err());
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM memory.knowledge_consolidation_decisions WHERE run_id = $1",
    )
    .bind(run)
    .fetch_one(fx.handle.pool())
    .await?;
    assert_eq!(count, 0);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn set_decision_application_stamps_applied_and_requires_error_for_failed() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let run = fx.run().await?;
    let knowledge = fx.handle.knowledge();
    let id = knowledge
        .write_consolidation_decisions(&[decision(run, project, "lesson:a", "covered")])
        .await?[0];

    assert!(knowledge.set_decision_application(id, DecisionApplication::Applied, None).await?);
    assert_eq!(stored(&fx, id).await?, ("applied".into(), None, true));
    assert!(knowledge.set_decision_application(id, DecisionApplication::Undone, None).await?);
    assert_eq!(stored(&fx, id).await?, ("undone".into(), None, true));
    assert!(knowledge.set_decision_application(id, DecisionApplication::Failed, Some("write refused")).await?);
    assert_eq!(stored(&fx, id).await?.1.as_deref(), Some("write refused"));

    assert!(knowledge.set_decision_application(id, DecisionApplication::Failed, None).await.is_err());
    assert!(knowledge.set_decision_application(id, DecisionApplication::Proposed, Some("x")).await.is_err());
    assert!(!knowledge.set_decision_application(i64::MAX, DecisionApplication::None, None).await?);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn decision_survives_deleting_the_statement_it_names() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let run = fx.run().await?;
    let lesson = fx.lesson(project, "Short lived.").await?;
    let statement_id = fx
        .statement_ids(lesson.item_id, "h", vec![statement("Short lived.", &vector(&[(50, 1.0)]))])
        .await?[0];
    let id = fx
        .handle
        .knowledge()
        .write_consolidation_decisions(&[NewDecision {
            statement_id: Some(statement_id),
            item_id: Some(lesson.item_id),
            ..decision(run, project, &lesson.reference, "covered")
        }])
        .await?[0];

    sqlx::query("DELETE FROM memory.knowledge_items WHERE id = $1")
        .bind(lesson.item_id)
        .execute(fx.handle.pool())
        .await?;
    let (statement, item, reference, before): (Option<i64>, Option<i64>, String, String) = sqlx::query_as(
        "SELECT statement_id, item_id, carrier_reference, text_before
           FROM memory.knowledge_consolidation_decisions WHERE id = $1",
    )
    .bind(id)
    .fetch_one(fx.handle.pool())
    .await?;
    assert_eq!((statement, item), (None, None));
    assert_eq!(reference, lesson.reference);
    assert_eq!(before, "Before.");
    fx.db.cleanup().await?;
    Ok(())
}
