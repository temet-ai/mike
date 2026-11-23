use super::llm::Failure;
use super::test_support::{harness, split};
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;

const BODY: &str = "# Rules\n\nUse jj for commits.\nNever force-push main.\nRun clippy first.\n";

#[tokio::test]
async fn a_lesson_becomes_one_statement_in_its_own_subject_and_the_item_is_done() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let project = h.fx.project().await?;
    let lesson = h.fx.lesson(project, "Use jj, not git.").await?;
    assert!(h.fx.consolidate_at(lesson.item_id).await?.is_some());

    let tally = h.items().await?;
    assert_eq!((tally.items_done, tally.statements_created, tally.subjects_created), (1, 1, 1));
    let rows = h.fx.statements_of(lesson.item_id).await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1, "Use jj, not git.");
    assert_eq!(rows[0].2, "current");
    assert!(rows[0].3.is_some());
    assert_eq!(h.fx.consolidate_at(lesson.item_id).await?, None);
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_requeued_lesson_with_the_same_text_adds_no_statement_and_does_not_mark_its_subject() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let project = h.fx.project().await?;
    let lesson = h.fx.lesson(project, "Use jj, not git.").await?;
    h.items().await?;
    h.subjects(None).await?;
    assert_eq!(h.backlog().await?, 0);

    h.fx.handle.knowledge().queue_knowledge_consolidation(&[lesson.item_id]).await?;
    let tally = h.items().await?;
    assert_eq!((tally.items_done, tally.statements_created), (1, 0));
    assert_eq!(h.fx.statements_of(lesson.item_id).await?.len(), 1);
    assert_eq!(h.backlog().await?, 0);
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_changed_lesson_replaces_its_statement_and_marks_the_subject() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let project = h.fx.project().await?;
    let lesson = h.fx.lesson(project, "Use jj.").await?;
    h.items().await?;
    h.subjects(None).await?;

    sqlx::query("UPDATE memory.lessons SET text = 'Use jj, never git.' WHERE id = $1")
        .bind(lesson.source_id)
        .execute(h.fx.handle.pool())
        .await?;
    h.fx.embed(&lesson, "Use jj, never git.").await?;
    h.items().await?;
    let rows = h.fx.statements_of(lesson.item_id).await?;
    let states: Vec<&str> = rows.iter().map(|r| r.2.as_str()).collect();
    assert_eq!(states, vec!["gone", "current"]);
    assert_eq!(rows[1].1, "Use jj, never git.");
    assert!(h.fx.handle.knowledge().consolidation_backlog().await?.subjects >= 1);
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_retired_lesson_has_its_statements_marked_gone() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let project = h.fx.project().await?;
    let lesson = h.fx.lesson(project, "Use jj.").await?;
    h.items().await?;
    sqlx::query("UPDATE memory.lessons SET retired_at = now(), retired_kind = 'consolidated', retired_reason = 'test' WHERE id = $1")
        .bind(lesson.source_id)
        .execute(h.fx.handle.pool())
        .await?;
    h.fx.handle.knowledge().queue_knowledge_consolidation(&[lesson.item_id]).await?;
    h.items().await?;
    let rows = h.fx.statements_of(lesson.item_id).await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].2, "gone");
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn close_statements_share_a_subject_and_distinct_ones_do_not() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    h.emb.set("Use jj.", &[(5, 1.0)]);
    h.emb.set("Use jj, never git.", &[(5, 0.99), (6, 0.1)]);
    h.emb.set("Run clippy.", &[(9, 1.0)]);
    let project = h.fx.project().await?;
    let a = h.fx.lesson(project, "Use jj.").await?;
    let b = h.fx.lesson(project, "Use jj, never git.").await?;
    let c = h.fx.lesson(project, "Run clippy.").await?;
    let tally = h.items().await?;
    assert_eq!((tally.subjects_created, tally.statements_created), (2, 3));
    let subject = |rows: &[(i64, String, String, Option<i64>)]| rows[0].3;
    let (sa, sb, sc) = (
        subject(&h.fx.statements_of(a.item_id).await?),
        subject(&h.fx.statements_of(b.item_id).await?),
        subject(&h.fx.statements_of(c.item_id).await?),
    );
    assert!(sa.is_some() && sa == sb);
    assert_ne!(sa, sc);
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_file_is_split_with_offsets_in_the_original_body_and_unfound_passages_are_dropped() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let project = h.fx.project().await?;
    let file = h
        .fx
        .checkout_file(project, "rules", "git.md", BODY, h.root.path())
        .await?;
    h.llm.push_split(
        0,
        Ok(split(&[
            ("Use jj for commits.", "Use  jj for\ncommits."),
            ("Never force-push main.", "Never force-push main."),
            ("Invented.", "This is nowhere in the file."),
        ])),
    );
    let tally = h.items().await?;
    assert_eq!(tally.statements_created, 2);
    let rows: Vec<(String, Option<i32>, Option<i32>)> = sqlx::query_as(
        "SELECT text, passage_start, passage_end FROM memory.knowledge_statements
          WHERE item_id = $1 ORDER BY ordinal",
    )
    .bind(file.carrier.item_id)
    .fetch_all(h.fx.handle.pool())
    .await?;
    assert_eq!(rows.len(), 2);
    let (Some(start), Some(end)) = (rows[1].1, rows[1].2) else {
        anyhow::bail!("no offsets recorded");
    };
    assert_eq!(&BODY[start as usize..end as usize], "Never force-push main.");
    assert_eq!(h.fx.consolidate_at(file.carrier.item_id).await?, None);

    h.fx.handle.knowledge().queue_knowledge_consolidation(&[file.carrier.item_id]).await?;
    h.items().await?;
    assert_eq!(h.llm.split_calls.lock().map(|c| c.len()).unwrap_or(0), 1);
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_split_that_finds_too_little_tries_the_next_model() -> anyhow::Result<()> {
    let Some(mut h) = harness(2).await? else {
        return Ok(());
    };
    let project = h.fx.project().await?;
    let file = h
        .fx
        .checkout_file(project, "rules", "git.md", BODY, h.root.path())
        .await?;
    h.llm.push_split(
        0,
        Ok(split(&[("A.", "Never force-push main."), ("B.", "nope one"), ("C.", "nope two")])),
    );
    h.llm.push_split(
        1,
        Ok(split(&[("Use jj for commits.", "Use jj for commits."), ("Run clippy first.", "Run clippy first.")])),
    );
    let tally = h.items().await?;
    assert_eq!((tally.items_done, tally.statements_created), (1, 2));
    let calls = h.llm.split_calls.lock().map(|c| c.clone()).unwrap_or_default();
    assert_eq!(calls, vec![0, 1]);
    assert_eq!(h.fx.statements_of(file.carrier.item_id).await?.len(), 2);
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn when_every_model_fails_the_item_is_deferred_with_a_growing_backoff() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let project = h.fx.project().await?;
    let file = h
        .fx
        .checkout_file(project, "rules", "git.md", BODY, h.root.path())
        .await?;
    h.llm.push_split(0, Err(Failure::transient("timeout")));
    let tally = h.items().await?;
    assert_eq!((tally.items_done, tally.items_deferred), (0, 1));
    let first = h.fx.consolidate_at(file.carrier.item_id).await?;
    let Some(first) = first else {
        anyhow::bail!("the item must stay queued");
    };
    assert!(first > chrono::Utc::now() + chrono::Duration::seconds(30));
    assert!(tally.last_error.as_deref().unwrap_or_default().contains("timeout"));

    sqlx::query("UPDATE memory.knowledge_items SET consolidate_at = now() WHERE id = $1")
        .bind(file.carrier.item_id)
        .execute(h.fx.handle.pool())
        .await?;
    h.llm.push_split(0, Err(Failure::transient("timeout")));
    h.items().await?;
    let second = h.fx.consolidate_at(file.carrier.item_id).await?;
    let Some(second) = second else {
        anyhow::bail!("the item must stay queued");
    };
    assert!(second > first + chrono::Duration::seconds(30));
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn an_embedding_failure_defers_the_lesson_and_a_permanent_one_waits_hours() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let project = h.fx.project().await?;
    let lesson = h.fx.lesson(project, "Use jj.").await?;
    h.emb.fail_with(Some(Failure::permanent("invalid embedding vector")));
    let tally = h.items().await?;
    assert_eq!(tally.items_deferred, 1);
    let Some(at) = h.fx.consolidate_at(lesson.item_id).await? else {
        anyhow::bail!("the item must stay queued");
    };
    assert!(at > chrono::Utc::now() + chrono::Duration::hours(5));
    assert!(h.fx.statements_of(lesson.item_id).await?.is_empty());
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn session_evidence_close_to_a_statement_marks_its_subject_changed() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    h.emb.set("Use jj.", &[(5, 1.0)]);
    let project = h.fx.project().await?;
    h.fx.lesson(project, "Use jj.").await?;
    h.items().await?;
    h.subjects(None).await?;
    assert_eq!(h.backlog().await?, 0);

    let far = h.fx.evidence("correction", project, "Unrelated.", &super::test_support::vector(&[(40, 1.0)])).await?;
    h.items().await?;
    assert_eq!(h.backlog().await?, 0);
    assert_eq!(h.fx.consolidate_at(far.item_id).await?, None);

    let near = h.fx.evidence("correction", project, "No, use jj.", &super::test_support::vector(&[(5, 1.0)])).await?;
    h.items().await?;
    assert_eq!(h.backlog().await?, 1);
    assert_eq!(h.fx.consolidate_at(near.item_id).await?, None);
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_non_canonical_twin_is_done_without_statements() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let project = h.fx.project().await?;
    let first = h.fx.checkout_file(project, "rules", "git.md", BODY, h.root.path()).await?;
    let second = h.fx.checkout_file(project, "rules", "git.md", BODY, h.root.path()).await?;
    h.llm.push_split(0, Ok(split(&[("Use jj for commits.", "Use jj for commits.")])));
    let tally = h.items().await?;
    assert_eq!(tally.items_done, 2);
    assert_eq!(h.llm.split_calls.lock().map(|c| c.len()).unwrap_or(0), 1);
    let (low, high) = if first.carrier.item_id < second.carrier.item_id {
        (&first, &second)
    } else {
        (&second, &first)
    };
    assert_eq!(h.fx.statements_of(low.carrier.item_id).await?.len(), 1);
    assert!(h.fx.statements_of(high.carrier.item_id).await?.is_empty());
    assert_eq!(h.fx.consolidate_at(high.carrier.item_id).await?, None);
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn other_kinds_and_non_guidance_files_are_done_without_work() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let project = h.fx.project().await?;
    h.fx.evidence("mail", project, "hello", &super::test_support::vector(&[(3, 1.0)])).await?;
    h.fx.checkout_file(project, "docs", "readme.md", "# Readme\n", h.root.path()).await?;
    let tally = h.items().await?;
    assert_eq!((tally.items_done, tally.statements_created), (2, 0));
    assert!(h.llm.split_calls.lock().map(|c| c.is_empty()).unwrap_or(false));
    h.fx.db.cleanup().await?;
    Ok(())
}
