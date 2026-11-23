use super::decisions_tests::decision;
use super::test_fixture::{fixture, statement, vector};
use super::{DecisionCursor, MeasureInput, NewDecision};
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use chrono::{Duration, Utc};

#[tokio::test]
async fn list_subjects_reports_members_carriers_and_newest_decision() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let (mine, theirs) = (fx.project().await?, fx.project().await?);
    let run = fx.run().await?;
    let (a, b, c) = (
        fx.lesson(mine, "A.").await?,
        fx.lesson(mine, "B.").await?,
        fx.lesson(theirs, "C.").await?,
    );
    let a_id = fx.statement_ids(a.item_id, "h", vec![statement("A.", &vector(&[(70, 1.0)]))]).await?[0];
    let b_id = fx.statement_ids(b.item_id, "h", vec![statement("B.", &vector(&[(70, 0.9)]))]).await?[0];
    let c_id = fx.statement_ids(c.item_id, "h", vec![statement("C.", &vector(&[(71, 1.0)]))]).await?[0];
    let knowledge = fx.handle.knowledge();
    let subject = knowledge.create_knowledge_subject(Some(mine), a_id, Utc::now()).await?;
    knowledge.add_statement_to_subject(b_id, subject, Utc::now()).await?;
    let foreign = knowledge.create_knowledge_subject(Some(theirs), c_id, Utc::now()).await?;
    knowledge
        .write_consolidation_decisions(&[
            NewDecision {
                subject_id: Some(subject),
                reason: "older".into(),
                ..decision(run, mine, &a.reference, "kept")
            },
            NewDecision {
                subject_id: Some(subject),
                reason: "newest".into(),
                ..decision(run, mine, &b.reference, "covered")
            },
        ])
        .await?;

    let listed = knowledge.list_knowledge_subjects(mine, 100, 0).await?;
    let found = listed.iter().find(|s| s.id == subject).ok_or_else(|| anyhow::anyhow!("missing"))?;
    assert_eq!((found.members, found.carriers), (2, 2));
    assert_eq!(found.newest_verdict.as_deref(), Some("covered"));
    assert_eq!(found.newest_reason.as_deref(), Some("newest"));
    assert!(found.changed_at.is_some());
    assert!(listed.iter().all(|s| s.id != foreign));
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn list_decisions_pages_newest_first_with_a_cursor_and_filters_by_subject() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let (mine, theirs) = (fx.project().await?, fx.project().await?);
    let run = fx.run().await?;
    let lesson = fx.lesson(mine, "A.").await?;
    let id = fx.statement_ids(lesson.item_id, "h", vec![statement("A.", &vector(&[(72, 1.0)]))]).await?[0];
    let knowledge = fx.handle.knowledge();
    let subject = knowledge.create_knowledge_subject(Some(mine), id, Utc::now()).await?;
    let mut batch: Vec<NewDecision> = (0..5)
        .map(|n| decision(run, mine, &format!("lesson:{n}"), "kept"))
        .collect();
    batch[3].subject_id = Some(subject);
    batch.push(decision(run, theirs, "lesson:foreign", "kept"));
    let ids = knowledge.write_consolidation_decisions(&batch).await?;
    sqlx::query(
        "UPDATE memory.knowledge_consolidation_decisions d SET created_at = now() - make_interval(secs => x.n)
           FROM unnest($1::bigint[], $2::int[]) AS x(id, n) WHERE d.id = x.id",
    )
    .bind(&ids)
    .bind((0..ids.len() as i32).map(|n| 100 - n * 10).collect::<Vec<_>>())
    .execute(fx.handle.pool())
    .await?;

    let first = knowledge.list_consolidation_decisions(mine, None, None, 2).await?;
    assert_eq!(first.iter().map(|d| d.carrier_reference.as_str()).collect::<Vec<_>>(), ["lesson:4", "lesson:3"]);
    let cursor = first.last().map(|d| DecisionCursor { created_at: d.created_at, id: d.id });
    let second = knowledge.list_consolidation_decisions(mine, None, cursor, 10).await?;
    assert_eq!(second.iter().map(|d| d.carrier_reference.as_str()).collect::<Vec<_>>(), ["lesson:2", "lesson:1", "lesson:0"]);
    assert_eq!(first[1].subject_id, Some(subject));

    let of_subject = knowledge.list_consolidation_decisions(mine, Some(subject), None, 10).await?;
    assert_eq!(of_subject.len(), 1);
    assert_eq!(of_subject[0].carrier_reference, "lesson:3");
    assert!(of_subject[0].subject_title.is_some());
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn list_measures_returns_the_series_oldest_first_since_a_time() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let knowledge = fx.handle.knowledge();
    for distance in [0.30_f32, 0.32, 0.34] {
        let run = fx.run().await?;
        knowledge
            .record_consolidation_measure(MeasureInput {
                run_id: run,
                project_id: Some(project),
                split_subject_share: 0.1,
                grouping_distance: distance,
            })
            .await?;
    }
    sqlx::query(
        "UPDATE memory.knowledge_consolidation_measures m SET measured_at = now() - make_interval(days => x.n)
           FROM (SELECT id, 3 - (row_number() OVER (ORDER BY id))::int AS n
                   FROM memory.knowledge_consolidation_measures WHERE project_id = $1) x
          WHERE m.id = x.id",
    )
    .bind(project)
    .execute(fx.handle.pool())
    .await?;

    let all = knowledge.list_consolidation_measures(Some(project), None, 10).await?;
    let distances: Vec<f32> = all.iter().map(|m| m.grouping_distance).collect();
    assert_eq!(distances, vec![0.30, 0.32, 0.34]);
    let recent = knowledge
        .list_consolidation_measures(Some(project), Some(Utc::now() - Duration::hours(36)), 10)
        .await?;
    assert_eq!(recent.iter().map(|m| m.grouping_distance).collect::<Vec<_>>(), vec![0.32, 0.34]);
    assert_eq!(knowledge.list_consolidation_measures(Some(project), None, 1).await?.len(), 1);
    fx.db.cleanup().await?;
    Ok(())
}
