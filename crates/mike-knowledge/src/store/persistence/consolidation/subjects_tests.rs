use super::test_fixture::{fixture, statement, vector};
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use chrono::{Duration, Utc};

async fn centre_distance(
    fx: &super::test_fixture::Fixture,
    subject: i64,
    probe: &str,
) -> anyhow::Result<f64> {
    Ok(sqlx::query_scalar(
        "SELECT (centre <=> $2::text::public.vector)::float8 FROM memory.knowledge_subjects WHERE id = $1",
    )
    .bind(subject)
    .bind(probe)
    .fetch_one(fx.handle.pool())
    .await?)
}

#[tokio::test]
async fn create_subject_centres_on_first_statement_and_marks_it_changed() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let lesson = fx.lesson(project, "Use jj.").await?;
    let id = fx.statement_ids(lesson.item_id, "h", vec![statement("Use jj.", &vector(&[(20, 1.0)]))]).await?[0];

    let subject = fx
        .handle
        .knowledge()
        .create_knowledge_subject(Some(project), id, Utc::now())
        .await?;
    assert!(centre_distance(&fx, subject, &vector(&[(20, 1.0)])).await? < 1e-6);
    let (members, changed, due): (i32, bool, bool) = sqlx::query_as(
        "SELECT members, changed_at IS NOT NULL, judge_after IS NOT NULL
           FROM memory.knowledge_subjects WHERE id = $1",
    )
    .bind(subject)
    .fetch_one(fx.handle.pool())
    .await?;
    assert_eq!((members, changed, due), (1, true, true));
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn add_and_remove_statement_recompute_centre_as_mean_of_members() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let (a, b) = (fx.lesson(project, "A.").await?, fx.lesson(project, "B.").await?);
    fx.embed(&a, "h", "A.").await?;
    fx.embed(&b, "h", "B.").await?;
    let a_id = fx.statement_ids(a.item_id, "h", vec![statement("A.", &vector(&[(21, 1.0)]))]).await?[0];
    let b_id = fx.statement_ids(b.item_id, "h", vec![statement("B.", &vector(&[(22, 1.0)]))]).await?[0];
    let knowledge = fx.handle.knowledge();
    let subject = knowledge.create_knowledge_subject(Some(project), a_id, Utc::now()).await?;

    knowledge.add_statement_to_subject(b_id, subject, Utc::now()).await?;
    let mean = vector(&[(21, 0.5), (22, 0.5)]);
    assert!(centre_distance(&fx, subject, &mean).await? < 1e-6);
    assert_eq!(knowledge.knowledge_subject_statements(subject).await?.len(), 2);

    knowledge.remove_statement_from_subject(b_id, Utc::now()).await?;
    assert!(centre_distance(&fx, subject, &vector(&[(21, 1.0)])).await? < 1e-6);
    let members: i32 = sqlx::query_scalar("SELECT members FROM memory.knowledge_subjects WHERE id = $1")
        .bind(subject)
        .fetch_one(fx.handle.pool())
        .await?;
    assert_eq!(members, 1);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn add_statement_to_subject_moves_it_out_of_its_previous_subject() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let (a, b) = (fx.lesson(project, "A.").await?, fx.lesson(project, "B.").await?);
    fx.embed(&a, "h", "A.").await?;
    fx.embed(&b, "h", "B.").await?;
    let a_id = fx.statement_ids(a.item_id, "h", vec![statement("A.", &vector(&[(23, 1.0)]))]).await?[0];
    let b_id = fx.statement_ids(b.item_id, "h", vec![statement("B.", &vector(&[(24, 1.0)]))]).await?[0];
    let knowledge = fx.handle.knowledge();
    let first = knowledge.create_knowledge_subject(Some(project), a_id, Utc::now()).await?;
    let second = knowledge.create_knowledge_subject(Some(project), b_id, Utc::now()).await?;

    knowledge.add_statement_to_subject(b_id, first, Utc::now()).await?;
    assert_eq!(knowledge.knowledge_subject_statements(first).await?.len(), 2);
    assert!(knowledge.knowledge_subject_statements(second).await?.is_empty());
    assert!(knowledge.drop_empty_knowledge_subjects().await? >= 1);
    let survivor: i64 = sqlx::query_scalar("SELECT count(*) FROM memory.knowledge_subjects WHERE id = $1")
        .bind(second)
        .fetch_one(fx.handle.pool())
        .await?;
    assert_eq!(survivor, 0);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn add_statement_to_unknown_statement_fails() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let result = fx
        .handle
        .knowledge()
        .add_statement_to_subject(i64::MAX, i64::MAX, Utc::now())
        .await;
    assert!(result.is_err());
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn claim_due_subjects_waits_for_judge_after_and_finish_needs_same_change() -> anyhow::Result<()> {
    let Some(fx) = fixture(true).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let (a, b) = (fx.lesson(project, "A.").await?, fx.lesson(project, "B.").await?);
    let a_id = fx.statement_ids(a.item_id, "h", vec![statement("A.", &vector(&[(25, 1.0)]))]).await?[0];
    let b_id = fx.statement_ids(b.item_id, "h", vec![statement("B.", &vector(&[(26, 1.0)]))]).await?[0];
    let knowledge = fx.handle.knowledge();
    let later = knowledge
        .create_knowledge_subject(Some(project), a_id, Utc::now() + Duration::hours(1))
        .await?;
    let due = knowledge
        .create_knowledge_subject(Some(project), b_id, Utc::now() - Duration::seconds(1))
        .await?;

    let claimed = knowledge.claim_due_knowledge_subjects(10).await?;
    assert!(claimed.iter().any(|s| s.id == due));
    assert!(claimed.iter().all(|s| s.id != later));
    let mine = claimed.iter().find(|s| s.id == due).ok_or_else(|| anyhow::anyhow!("missing"))?;

    knowledge.mark_knowledge_subjects_changed(&[due], Utc::now() - Duration::seconds(1)).await?;
    assert!(
        !knowledge
            .finish_knowledge_subject_judgement(due, mine.changed_at, Some(("Title", "Text")))
            .await?
    );
    let again = knowledge.claim_due_knowledge_subjects(10).await?;
    let fresh = again.iter().find(|s| s.id == due).ok_or_else(|| anyhow::anyhow!("missing"))?;
    assert!(
        knowledge
            .finish_knowledge_subject_judgement(due, fresh.changed_at, Some(("Title", "Text")))
            .await?
    );
    let (title, text, cleared, judged): (String, String, bool, bool) = sqlx::query_as(
        "SELECT title, text, changed_at IS NULL AND judge_after IS NULL, judged_at IS NOT NULL
           FROM memory.knowledge_subjects WHERE id = $1",
    )
    .bind(due)
    .fetch_one(fx.handle.pool())
    .await?;
    assert_eq!((title.as_str(), text.as_str(), cleared, judged), ("Title", "Text", true, true));
    assert!(knowledge.claim_due_knowledge_subjects(10).await?.iter().all(|s| s.id != due));
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn subject_statements_carry_reference_kind_source_name_and_date() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let lesson = fx.lesson(project, "Lesson text.").await?;
    let file = fx.checkout_file(project, "skills", "skill.md", "File text.").await?;
    fx.embed(&lesson, "h", "Lesson text.").await?;
    fx.embed(&file, "h", "File text.").await?;
    let l_id = fx.statement_ids(lesson.item_id, "h", vec![statement("Lesson text.", &vector(&[(27, 1.0)]))]).await?[0];
    let f_id = fx.statement_ids(file.item_id, "h", vec![statement("File text.", &vector(&[(27, 0.9), (28, 0.1)]))]).await?[0];
    let knowledge = fx.handle.knowledge();
    let subject = knowledge.create_knowledge_subject(Some(project), l_id, Utc::now()).await?;
    knowledge.add_statement_to_subject(f_id, subject, Utc::now()).await?;

    let loaded = knowledge.knowledge_subject_statements(subject).await?;
    assert_eq!(loaded.len(), 2);
    let lesson_row = loaded.iter().find(|s| s.statement_id == l_id).ok_or_else(|| anyhow::anyhow!("missing"))?;
    let file_row = loaded.iter().find(|s| s.statement_id == f_id).ok_or_else(|| anyhow::anyhow!("missing"))?;
    assert_eq!(lesson_row.carrier_reference, lesson.reference);
    assert_eq!(lesson_row.kind, "lesson");
    assert_eq!(lesson_row.source_name, None);
    assert_eq!(file_row.carrier_reference, file.reference);
    assert_eq!(file_row.kind, "artifact");
    assert_eq!(file_row.source_name.as_deref(), Some("skills"));
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn nearest_subjects_stay_in_the_same_project_and_skip_self_and_empty() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let (mine, theirs) = (fx.project().await?, fx.project().await?);
    let mut subjects = Vec::new();
    for (project, axis, weight) in [(mine, 30, 1.0), (mine, 30, 0.5), (mine, 31, 1.0), (theirs, 30, 1.0)] {
        let lesson = fx.lesson(project, &format!("Lesson {axis} {weight}.")).await?;
        let spikes = if weight < 1.0 { vec![(axis, 1.0), (axis + 5, 1.0)] } else { vec![(axis, 1.0)] };
        let id = fx.statement_ids(lesson.item_id, "h", vec![statement("x", &vector(&spikes))]).await?[0];
        subjects.push(fx.handle.knowledge().create_knowledge_subject(Some(project), id, Utc::now()).await?);
    }

    let near = fx.handle.knowledge().nearest_knowledge_subjects(subjects[0], 10).await?;
    let ids: Vec<i64> = near.iter().map(|n| n.subject_id).collect();
    assert_eq!(ids.first(), Some(&subjects[1]));
    assert!(ids.contains(&subjects[2]));
    assert!(!ids.contains(&subjects[0]) && !ids.contains(&subjects[3]));
    assert!(near[0].similarity > near[1].similarity);
    fx.db.cleanup().await?;
    Ok(())
}
