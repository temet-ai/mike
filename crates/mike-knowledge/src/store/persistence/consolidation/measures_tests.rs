use super::decisions_tests::decision;
use super::test_fixture::{fixture, statement, vector};
use super::{MeasureInput, NewDecision};
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use chrono::Utc;

#[tokio::test]
async fn record_measure_counts_statements_subjects_shared_conflicts_and_characters() -> anyhow::Result<()> {
    let Some(fx) = fixture(true).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let run = fx.run().await?;
    let file = fx.checkout_file(project, "rules", "a.md", "Alpha. Beta.").await?;
    let lesson = fx.lesson(project, "Gamma.").await?;
    let solo = fx.lesson(project, "Delta.").await?;
    let file_ids = fx
        .statement_ids(
            file.item_id,
            "h",
            vec![statement("Alpha.", &vector(&[(60, 1.0)])), statement("Beta.", &vector(&[(61, 1.0)]))],
        )
        .await?;
    let lesson_id = fx.statement_ids(lesson.item_id, "h", vec![statement("Gamma.", &vector(&[(60, 0.9)]))]).await?[0];
    let solo_id = fx.statement_ids(solo.item_id, "h", vec![statement("Delta.", &vector(&[(62, 1.0)]))]).await?[0];
    let knowledge = fx.handle.knowledge();
    let shared = knowledge.create_knowledge_subject(Some(project), file_ids[0], Utc::now()).await?;
    knowledge.add_statement_to_subject(lesson_id, shared, Utc::now()).await?;
    knowledge.create_knowledge_subject(Some(project), file_ids[1], Utc::now()).await?;
    knowledge.create_knowledge_subject(Some(project), solo_id, Utc::now()).await?;

    let conflict = |reference: &str, verdict: &str, a: i64, b: i64| NewDecision {
        statement_id: Some(a),
        counterpart_statement_id: Some(b),
        ..decision(run, project, reference, verdict)
    };
    knowledge
        .write_consolidation_decisions(&[
            conflict("a", "conflict_open", file_ids[0], lesson_id),
            conflict("b", "conflict_open", lesson_id, file_ids[0]),
            conflict("c", "misgrouped", solo_id, lesson_id),
            conflict("d", "kept", file_ids[1], solo_id),
        ])
        .await?;

    let row = knowledge
        .record_consolidation_measure(MeasureInput {
            run_id: run,
            project_id: Some(project),
            split_subject_share: 0.25,
            grouping_distance: 0.31,
        })
        .await?;
    assert_eq!(row.statements_current, 4);
    assert_eq!(row.subjects, 3);
    assert_eq!(row.subjects_shared, 1);
    assert_eq!(row.conflicts_open, 1);
    assert_eq!(row.guidance_chars, ("Alpha.".len() + "Beta.".len() + "Gamma.".len() + "Delta.".len()) as i64);
    assert!((row.misgrouped_share - 0.25).abs() < 1e-6);
    assert!((row.split_subject_share - 0.25).abs() < 1e-6);
    assert!((row.grouping_distance - 0.31).abs() < 1e-6);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn record_measure_for_empty_project_is_all_zero_and_rejects_out_of_range_distance() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let run = fx.run().await?;
    let knowledge = fx.handle.knowledge();
    let row = knowledge
        .record_consolidation_measure(MeasureInput {
            run_id: run,
            project_id: Some(project),
            split_subject_share: 0.0,
            grouping_distance: 0.4,
        })
        .await?;
    assert_eq!((row.statements_current, row.subjects, row.conflicts_open, row.guidance_chars), (0, 0, 0, 0));
    assert_eq!(row.misgrouped_share, 0.0);

    let bad = knowledge
        .record_consolidation_measure(MeasureInput {
            run_id: run,
            project_id: Some(project),
            split_subject_share: 1.5,
            grouping_distance: 0.4,
        })
        .await;
    assert!(bad.is_err());
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn latest_grouping_distance_returns_newest_measure_or_none() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let knowledge = fx.handle.knowledge();
    assert_eq!(knowledge.latest_grouping_distance(Some(project)).await?, None);
    for distance in [0.30_f32, 0.34] {
        let run = fx.run().await?;
        knowledge
            .record_consolidation_measure(MeasureInput {
                run_id: run,
                project_id: Some(project),
                split_subject_share: 0.0,
                grouping_distance: distance,
            })
            .await?;
    }
    assert_eq!(knowledge.latest_grouping_distance(Some(project)).await?, Some(0.34));
    fx.db.cleanup().await?;
    Ok(())
}
