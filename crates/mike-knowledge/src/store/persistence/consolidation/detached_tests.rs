use super::test_fixture::{Carrier, Fixture, fixture, statement, statements_of, vector};
use super::NewStatement;
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use chrono::{Duration, Utc};

fn passage(body: &str, text: &str, from: usize) -> (i32, i32) {
    let start = body[from..].find(text).map_or(0, |at| at + from);
    (start as i32, (start + text.len()) as i32)
}

async fn split(fx: &Fixture, file: &Carrier, body: &str, parts: &[&str]) -> anyhow::Result<Vec<i64>> {
    let mut from = 0;
    let mut items = Vec::new();
    for (n, part) in parts.iter().enumerate() {
        let (start, end) = passage(body, part, from);
        from = end as usize;
        items.push(NewStatement {
            passage_start: Some(start),
            passage_end: Some(end),
            ..statement(part, &vector(&[(100 + n, 1.0)]))
        });
    }
    fx.statement_ids(file.item_id, "h", items).await
}

async fn row(fx: &Fixture, id: i64) -> anyhow::Result<(Option<i64>, String, Option<i32>, Option<i32>, Option<i64>)> {
    Ok(sqlx::query_as(
        "SELECT item_id, state, passage_start, passage_end, subject_id FROM memory.knowledge_statements WHERE id = $1",
    )
    .bind(id)
    .fetch_one(fx.handle.pool())
    .await?)
}

const V1: &str = "Alpha rule.\nBeta rule.\nGamma rule.\n";

#[tokio::test]
async fn new_revision_detaches_statements_keeping_subjects_untouched() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let file = fx.checkout_file(project, "rules", "a.md", V1).await?;
    let ids = split(&fx, &file, V1, &["Alpha rule.", "Beta rule."]).await?;
    let knowledge = fx.handle.knowledge();
    let subject = knowledge.create_knowledge_subject(Some(project), ids[0], Utc::now()).await?;
    let doc = fx.document_of(&file).await?;
    let changed_before: Option<chrono::DateTime<Utc>> =
        sqlx::query_scalar("SELECT changed_at FROM memory.knowledge_subjects WHERE id = $1")
            .bind(subject)
            .fetch_one(fx.handle.pool())
            .await?;

    let next = fx.revise_file(&file, &format!("{V1}Delta rule.\n")).await?;
    assert_ne!(next.item_id, file.item_id);
    let (item, state, ..) = row(&fx, ids[0]).await?;
    assert_eq!((item, state.as_str()), (None, "current"));
    let found: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM memory.knowledge_statements WHERE document_uuid = $1 AND item_id IS NULL AND detached_at IS NOT NULL",
    )
    .bind(doc)
    .fetch_one(fx.handle.pool())
    .await?;
    assert_eq!(found, 2);
    let changed_after: Option<chrono::DateTime<Utc>> =
        sqlx::query_scalar("SELECT changed_at FROM memory.knowledge_subjects WHERE id = $1")
            .bind(subject)
            .fetch_one(fx.handle.pool())
            .await?;
    assert_eq!(changed_before, changed_after);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn detached_statements_are_not_guidance_for_nearest_subject_and_measure_reads() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let file = fx.checkout_file(project, "rules", "a.md", V1).await?;
    fx.embed(&file, "h", "x").await?;
    let ids = split(&fx, &file, V1, &["Alpha rule."]).await?;
    let knowledge = fx.handle.knowledge();
    let subject = knowledge.create_knowledge_subject(Some(project), ids[0], Utc::now()).await?;
    assert_eq!(knowledge.knowledge_subject_statements(subject).await?.len(), 1);

    fx.revise_file(&file, &format!("{V1}More.\n")).await?;
    assert!(knowledge.knowledge_subject_statements(subject).await?.is_empty());
    let near = knowledge.nearest_statements(Some(project), &vector(&[(100, 1.0)]), &[], 50).await?;
    assert!(near.iter().all(|n| n.statement_id != ids[0]));
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn rehome_keeps_identity_moves_passages_and_reports_uncovered_ranges() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let file = fx.checkout_file(project, "rules", "a.md", V1).await?;
    let ids = split(&fx, &file, V1, &["Alpha rule.", "Gamma rule."]).await?;
    let knowledge = fx.handle.knowledge();
    let subject = knowledge.create_knowledge_subject(Some(project), ids[0], Utc::now()).await?;
    sqlx::query("UPDATE memory.knowledge_statements SET stated_at = '2020-01-01Z' WHERE id = $1")
        .bind(ids[0])
        .execute(fx.handle.pool())
        .await?;

    let v2 = "Intro line.\nAlpha rule.\nNew rule.\nGamma rule.\n";
    let next = fx.revise_file(&file, v2).await?;
    let result = knowledge.rehome_document_statements(next.item_id, v2, Utc::now()).await?;

    assert_eq!(result.gone, 0);
    let (alpha, gamma) = (passage(v2, "Alpha rule.", 0), passage(v2, "Gamma rule.", 0));
    assert_eq!(
        result.rehomed.iter().map(|(id, r)| (*id, r.start, r.end)).collect::<Vec<_>>(),
        vec![(ids[0], alpha.0, alpha.1), (ids[1], gamma.0, gamma.1)]
    );
    let gaps: Vec<(i32, i32)> = result.uncovered.iter().map(|r| (r.start, r.end)).collect();
    assert_eq!(gaps, vec![(0, alpha.0), (alpha.1, gamma.0), (gamma.1, v2.len() as i32)]);
    let (item, state, start, end, subj) = row(&fx, ids[0]).await?;
    assert_eq!((item, state.as_str(), start, end, subj), (Some(next.item_id), "current", Some(alpha.0), Some(alpha.1), Some(subject)));
    let (stated, text, ordinal): (chrono::DateTime<Utc>, String, i32) = sqlx::query_as(
        "SELECT stated_at, text, ordinal FROM memory.knowledge_statements WHERE id = $1",
    )
    .bind(ids[0])
    .fetch_one(fx.handle.pool())
    .await?;
    assert_eq!((stated.to_rfc3339().as_str(), text.as_str(), ordinal), ("2020-01-01T00:00:00+00:00", "Alpha rule.", 0));
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn rehome_marks_missing_passages_gone_and_changes_their_subjects() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let file = fx.checkout_file(project, "rules", "a.md", V1).await?;
    let ids = split(&fx, &file, V1, &["Alpha rule.", "Beta rule."]).await?;
    let knowledge = fx.handle.knowledge();
    let subject = knowledge.create_knowledge_subject(Some(project), ids[1], Utc::now()).await?;
    let changed: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT changed_at FROM memory.knowledge_subjects WHERE id = $1")
            .bind(subject)
            .fetch_one(fx.handle.pool())
            .await?;
    assert!(knowledge.finish_knowledge_subject_judgement(subject, changed, None).await?);

    let v2 = "Alpha rule.\nBeta rule changed.\n";
    let next = fx.revise_file(&file, v2).await?;
    let result = knowledge.rehome_document_statements(next.item_id, v2, Utc::now() - Duration::seconds(1)).await?;
    assert_eq!(result.gone, 1);
    assert_eq!(result.rehomed.len(), 1);
    assert_eq!(row(&fx, ids[1]).await?.1, "gone");
    let (members, is_changed): (i32, bool) = sqlx::query_as(
        "SELECT members, changed_at IS NOT NULL FROM memory.knowledge_subjects WHERE id = $1",
    )
    .bind(subject)
    .fetch_one(fx.handle.pool())
    .await?;
    assert_eq!((members, is_changed), (0, true));
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn rehome_matches_in_order_and_whitespace_exactly() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let v1 = "Keep it. Keep it.\nOne  two.\n";
    let file = fx.checkout_file(project, "rules", "a.md", v1).await?;
    let ids = split(&fx, &file, v1, &["Keep it.", "Keep it.", "One  two."]).await?;
    let v2 = "Keep it.\nOne two.\nKeep it.\n";
    let next = fx.revise_file(&file, v2).await?;
    let result = fx.handle.knowledge().rehome_document_statements(next.item_id, v2, Utc::now()).await?;
    assert_eq!(result.rehomed.len(), 2);
    assert_eq!(result.gone, 1);
    assert_eq!(row(&fx, ids[2]).await?.1, "gone");
    let starts: Vec<i32> = result.rehomed.iter().map(|(_, r)| r.start).collect();
    assert_eq!(starts, vec![0, v2.rfind("Keep it.").map_or(0, |n| n as i32)]);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn rehome_counts_byte_offsets_for_multibyte_text() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let v1 = "Käse rule.\n";
    let file = fx.checkout_file(project, "rules", "a.md", v1).await?;
    split(&fx, &file, v1, &["Käse rule."]).await?;
    let v2 = "Ünï.\nKäse rule.\n";
    let next = fx.revise_file(&file, v2).await?;
    let result = fx.handle.knowledge().rehome_document_statements(next.item_id, v2, Utc::now()).await?;
    let (start, end) = passage(v2, "Käse rule.", 0);
    assert_eq!((result.rehomed[0].1.start, result.rehomed[0].1.end), (start, end));
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn rehome_for_a_lesson_item_or_without_detached_statements_returns_nothing() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let lesson = fx.lesson(project, "A lesson.").await?;
    let file = fx.checkout_file(project, "rules", "a.md", V1).await?;
    let knowledge = fx.handle.knowledge();
    assert_eq!(knowledge.rehome_document_statements(lesson.item_id, "A lesson.", Utc::now()).await?.rehomed.len(), 0);
    let empty = knowledge.rehome_document_statements(file.item_id, V1, Utc::now()).await?;
    assert_eq!((empty.rehomed.len(), empty.gone), (0, 0));
    assert_eq!(empty.uncovered.len(), 1);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn deleting_a_document_makes_detached_statements_gone_and_marks_subjects() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let file = fx.checkout_file(project, "rules", "a.md", V1).await?;
    let ids = split(&fx, &file, V1, &["Alpha rule."]).await?;
    let knowledge = fx.handle.knowledge();
    let subject = knowledge.create_knowledge_subject(Some(project), ids[0], Utc::now()).await?;
    let changed: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT changed_at FROM memory.knowledge_subjects WHERE id = $1")
            .bind(subject)
            .fetch_one(fx.handle.pool())
            .await?;
    knowledge.finish_knowledge_subject_judgement(subject, changed, None).await?;
    let doc = fx.document_of(&file).await?;
    fx.revise_file(&file, "Other.\n").await?;
    assert_eq!(row(&fx, ids[0]).await?.1, "current");

    sqlx::query("UPDATE memory.knowledge_documents SET deleted_at = now() WHERE document_uuid = $1")
        .bind(doc)
        .execute(fx.handle.pool())
        .await?;
    assert_eq!(row(&fx, ids[0]).await?.1, "gone");
    let is_changed: bool = sqlx::query_scalar("SELECT changed_at IS NOT NULL FROM memory.knowledge_subjects WHERE id = $1")
        .bind(subject)
        .fetch_one(fx.handle.pool())
        .await?;
    assert!(is_changed);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn deleting_an_item_of_a_deleted_document_and_a_lesson_still_cascades() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let file = fx.checkout_file(project, "rules", "a.md", V1).await?;
    let lesson = fx.lesson(project, "Lesson.").await?;
    let file_ids = split(&fx, &file, V1, &["Alpha rule."]).await?;
    let lesson_ids = fx.statement_ids(lesson.item_id, "h", vec![statement("Lesson.", &vector(&[(120, 1.0)]))]).await?;
    let doc = fx.document_of(&file).await?;
    sqlx::query("UPDATE memory.knowledge_documents SET deleted_at = now() WHERE document_uuid = $1")
        .bind(doc)
        .execute(fx.handle.pool())
        .await?;
    sqlx::query("DELETE FROM memory.knowledge_items WHERE id = $1").bind(lesson.item_id).execute(fx.handle.pool()).await?;
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM memory.knowledge_statements WHERE id = ANY($1)")
        .bind([file_ids[0], lesson_ids[0]].to_vec())
        .fetch_one(fx.handle.pool())
        .await?;
    assert_eq!(left, 0);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn add_item_statements_continues_ordinals_and_leaves_existing_current() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let file = fx.checkout_file(project, "rules", "a.md", V1).await?;
    let first = split(&fx, &file, V1, &["Alpha rule.", "Beta rule."]).await?;
    let added = fx
        .handle
        .knowledge()
        .add_item_statements(file.item_id, &statements_of("h2", vec![statement("Gamma rule.", &vector(&[(130, 1.0)])), statement("Delta.", &vector(&[(131, 1.0)]))]))
        .await?;
    assert_eq!(added.iter().map(|c| c.ordinal).collect::<Vec<_>>(), vec![2, 3]);
    assert_eq!(row(&fx, first[0]).await?.1, "current");
    let (doc, rev): (Option<uuid::Uuid>, Option<i64>) = sqlx::query_as(
        "SELECT document_uuid, revision_id FROM memory.knowledge_statements WHERE id = $1",
    )
    .bind(added[0].id)
    .fetch_one(fx.handle.pool())
    .await?;
    assert_eq!((doc, rev), (Some(fx.document_of(&file).await?), Some(file.source_id)));
    let bad = fx
        .handle
        .knowledge()
        .add_item_statements(file.item_id, &statements_of("h3", vec![NewStatement { passage_start: Some(5), passage_end: Some(1), ..statement("Bad.", &vector(&[(132, 1.0)])) }]))
        .await;
    assert!(bad.is_err());
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn sweep_marks_only_old_detached_statements_gone_and_changes_subjects() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let (old_file, new_file) = (
        fx.checkout_file(project, "rules", "a.md", V1).await?,
        fx.checkout_file(project, "rules", "b.md", V1).await?,
    );
    let old_ids = split(&fx, &old_file, V1, &["Alpha rule."]).await?;
    let new_ids = split(&fx, &new_file, V1, &["Beta rule."]).await?;
    let lesson = fx.lesson(project, "Live lesson.").await?;
    let live = fx.statement_ids(lesson.item_id, "h", vec![statement("Live lesson.", &vector(&[(140, 1.0)]))]).await?;
    let knowledge = fx.handle.knowledge();
    let subject = knowledge.create_knowledge_subject(Some(project), old_ids[0], Utc::now()).await?;
    let changed: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT changed_at FROM memory.knowledge_subjects WHERE id = $1")
            .bind(subject)
            .fetch_one(fx.handle.pool())
            .await?;
    knowledge.finish_knowledge_subject_judgement(subject, changed, None).await?;
    fx.revise_file(&old_file, "Other.\n").await?;
    fx.revise_file(&new_file, "Other.\n").await?;

    assert_eq!(knowledge.sweep_detached_statements(Utc::now()).await?, 0);
    sqlx::query("UPDATE memory.knowledge_statements SET detached_at = now() - interval '31 minutes' WHERE id = $1")
        .bind(old_ids[0])
        .execute(fx.handle.pool())
        .await?;
    assert_eq!(knowledge.sweep_detached_statements(Utc::now()).await?, 1);
    assert_eq!(row(&fx, old_ids[0]).await?.1, "gone");
    assert_eq!(row(&fx, new_ids[0]).await?.1, "current");
    assert_eq!(row(&fx, live[0]).await?.1, "current");
    let is_changed: bool = sqlx::query_scalar("SELECT changed_at IS NOT NULL FROM memory.knowledge_subjects WHERE id = $1")
        .bind(subject)
        .fetch_one(fx.handle.pool())
        .await?;
    assert!(is_changed);
    assert_eq!(knowledge.sweep_detached_statements(Utc::now()).await?, 0);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn claim_reports_detached_statements_for_the_new_revision_only() -> anyhow::Result<()> {
    let Some(fx) = fixture(true).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let file = fx.checkout_file(project, "rules", "a.md", V1).await?;
    let plain = fx.checkout_file(project, "rules", "b.md", V1).await?;
    fx.embed(&file, "h", "x").await?;
    fx.embed(&plain, "h", "x").await?;
    split(&fx, &file, V1, &["Alpha rule."]).await?;
    let next = fx.revise_file(&file, "Alpha rule.\nMore.\n").await?;
    fx.embed(&next, "h2", "x").await?;

    let claimed = fx.handle.knowledge().claim_due_knowledge_items(10).await?;
    let flag = |id: i64| claimed.iter().find(|c| c.item_id == id).map(|c| c.detached_statements);
    assert_eq!(flag(next.item_id), Some(true));
    assert_eq!(flag(plain.item_id), Some(false));
    fx.db.cleanup().await?;
    Ok(())
}
