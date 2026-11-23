use super::test_fixture::{fixture, statement, vector};
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;

#[tokio::test]
async fn details_item_statements_and_chunks_describe_a_lesson_and_a_file() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let lesson = fx.lesson(project, "Use jj.").await?;
    let file = fx
        .checkout_file(project, "rules", "a.md", "Use jj.\nNever git.\n")
        .await?;
    let lesson_ids = fx
        .statement_ids(lesson.item_id, "h1", vec![statement("Use jj.", &vector(&[(1, 1.0)]))])
        .await?;
    let mut file_statement = statement("Never git.", &vector(&[(2, 1.0)]));
    file_statement.passage_start = Some(8);
    file_statement.passage_end = Some(18);
    let file_ids = fx.statement_ids(file.item_id, "h2", vec![file_statement]).await?;

    let knowledge = fx.handle.knowledge();
    let all: Vec<i64> = lesson_ids.iter().chain(&file_ids).copied().collect();
    let details = knowledge.statement_details(&all).await?;
    assert_eq!(details.len(), 2);
    let lesson_detail = details
        .iter()
        .find(|d| d.kind == "lesson")
        .ok_or_else(|| anyhow::anyhow!("no lesson detail"))?;
    assert_eq!(lesson_detail.source_id, lesson.source_id);
    assert_eq!(lesson_detail.lesson_active, Some(true));
    let file_detail = details
        .iter()
        .find(|d| d.kind == "artifact")
        .ok_or_else(|| anyhow::anyhow!("no file detail"))?;
    assert_eq!(file_detail.source_name.as_deref(), Some("rules"));
    assert_eq!(file_detail.passage_start, Some(8));
    assert_eq!(file_detail.passage.as_deref(), Some("Never git."));
    assert_eq!(file_detail.lesson_active, None);

    let current = knowledge.item_current_statements(lesson.item_id).await?;
    assert_eq!(current.len(), 1);
    assert_eq!(current[0].split_hash, "h1");

    let locations = knowledge.knowledge_file_locations(&[file.item_id, lesson.item_id]).await?;
    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].relative_path, "a.md");
    assert!(locations[0].machine_uuid.is_some());

    assert_eq!(knowledge.lesson_id_of_item(lesson.item_id).await?, Some(lesson.source_id));
    assert_eq!(knowledge.lesson_id_of_item(file.item_id).await?, None);

    fx.chunk(&lesson, "Use jj.", &vector(&[(1, 1.0)])).await?;
    assert_eq!(knowledge.item_chunk_embeddings(lesson.item_id).await?.len(), 1);
    assert!(knowledge.consolidation_backlog().await?.items >= 0);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn judgements_count_distinct_subjects_per_run_since_a_time() -> anyhow::Result<()> {
    let Some(fx) = fixture(true).await? else {
        return Ok(());
    };
    let knowledge = fx.handle.knowledge();
    let since = chrono::Utc::now() - chrono::Duration::hours(1);
    assert_eq!(knowledge.consolidation_judgements_since(since).await?, 0);
    fx.db.cleanup().await?;
    Ok(())
}
