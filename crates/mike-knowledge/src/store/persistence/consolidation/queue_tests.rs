use super::test_fixture::{fixture, statement, vector};
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use chrono::{Duration, Utc};

#[tokio::test]
async fn save_embeddings_with_changed_hash_queues_item_and_unchanged_hash_does_not()
-> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let lesson = fx.lesson(project, "Run clippy before commit.").await?;
    assert_eq!(fx.consolidate_at(lesson.item_id).await?, None);

    fx.embed(&lesson, "hash-one", "Run clippy before commit.").await?;
    let queued = fx.consolidate_at(lesson.item_id).await?;
    assert!(queued.is_some());

    let knowledge = fx.handle.knowledge();
    assert!(
        knowledge
            .mark_knowledge_item_consolidated(lesson.item_id, queued.unwrap_or_default())
            .await?
    );
    fx.embed(&lesson, "hash-one", "Run clippy before commit.").await?;
    assert_eq!(fx.consolidate_at(lesson.item_id).await?, None);

    fx.embed(&lesson, "hash-two", "Run clippy and tests before commit.").await?;
    assert!(fx.consolidate_at(lesson.item_id).await?.is_some());
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn claim_due_items_returns_lesson_with_body_hash_and_active_flag() -> anyhow::Result<()> {
    let Some(fx) = fixture(true).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let lesson = fx.lesson(project, "Never force-push main.").await?;
    fx.embed(&lesson, "lesson-hash", "Never force-push main.").await?;

    let claimed = fx.handle.knowledge().claim_due_knowledge_items(10).await?;
    let item = claimed
        .iter()
        .find(|c| c.item_id == lesson.item_id)
        .ok_or_else(|| anyhow::anyhow!("lesson item not claimed"))?;
    assert_eq!(item.kind, "lesson");
    assert_eq!(item.body.as_deref(), Some("Never force-push main."));
    assert_eq!(item.source_hash, "lesson-hash");
    assert_eq!(item.project_id, Some(project));
    assert_eq!(item.canonical_item_id, lesson.item_id);
    assert_eq!(item.reference, lesson.reference);
    assert!(item.active);
    assert!(item.source_name.is_none());
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn claim_due_items_reports_retired_lesson_inactive_and_honours_limit() -> anyhow::Result<()> {
    let Some(fx) = fixture(true).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let retired = fx.lesson(project, "Old advice.").await?;
    let live = fx.lesson(project, "Fresh advice.").await?;
    fx.embed(&retired, "h1", "Old advice.").await?;
    fx.embed(&live, "h2", "Fresh advice.").await?;
    sqlx::query(
        "UPDATE memory.lessons SET retired_at = now(), retired_kind = 'outcome', retired_reason = 'x'
          WHERE id = $1",
    )
    .bind(retired.source_id)
    .execute(fx.handle.pool())
    .await?;

    let all = fx.handle.knowledge().claim_due_knowledge_items(10).await?;
    let flag = |id: i64| all.iter().find(|c| c.item_id == id).map(|c| c.active);
    assert_eq!(flag(retired.item_id), Some(false));
    assert_eq!(flag(live.item_id), Some(true));
    assert_eq!(fx.handle.knowledge().claim_due_knowledge_items(1).await?.len(), 1);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn claim_due_items_skips_future_deferred_and_unembedded_items() -> anyhow::Result<()> {
    let Some(fx) = fixture(true).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let deferred = fx.lesson(project, "Deferred.").await?;
    let waiting = fx.lesson(project, "Waiting for embedding.").await?;
    fx.embed(&deferred, "h1", "Deferred.").await?;
    fx.embed(&waiting, "h2", "Waiting for embedding.").await?;
    let knowledge = fx.handle.knowledge();
    assert!(
        knowledge
            .defer_knowledge_item_consolidation(deferred.item_id, Utc::now() + Duration::hours(1))
            .await?
    );
    sqlx::query("UPDATE memory.knowledge_items SET needs_embedding = true WHERE id = $1")
        .bind(waiting.item_id)
        .execute(fx.handle.pool())
        .await?;

    let claimed = knowledge.claim_due_knowledge_items(10).await?;
    assert!(claimed.iter().all(|c| c.item_id != deferred.item_id));
    assert!(claimed.iter().all(|c| c.item_id != waiting.item_id));
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn mark_item_consolidated_keeps_queue_when_item_changed_after_claim() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let lesson = fx.lesson(project, "Use jj.").await?;
    fx.embed(&lesson, "h1", "Use jj.").await?;
    let first = fx
        .consolidate_at(lesson.item_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("not queued"))?;

    fx.embed(&lesson, "h2", "Use jj always.").await?;
    let knowledge = fx.handle.knowledge();
    assert!(!knowledge.mark_knowledge_item_consolidated(lesson.item_id, first).await?);
    assert!(fx.consolidate_at(lesson.item_id).await?.is_some());
    let second = fx
        .consolidate_at(lesson.item_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("not queued"))?;
    assert!(knowledge.mark_knowledge_item_consolidated(lesson.item_id, second).await?);
    assert_eq!(fx.consolidate_at(lesson.item_id).await?, None);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn claim_due_items_skips_rows_locked_by_another_transaction() -> anyhow::Result<()> {
    let Some(fx) = fixture(true).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let locked = fx.lesson(project, "Locked.").await?;
    let free = fx.lesson(project, "Free.").await?;
    fx.embed(&locked, "h1", "Locked.").await?;
    fx.embed(&free, "h2", "Free.").await?;

    let mut tx = fx.handle.pool().begin().await?;
    sqlx::query("SELECT id FROM memory.knowledge_items WHERE id = $1 FOR UPDATE")
        .bind(locked.item_id)
        .fetch_one(&mut *tx)
        .await?;
    let claimed = fx.handle.knowledge().claim_due_knowledge_items(10).await?;
    tx.rollback().await?;

    assert!(claimed.iter().any(|c| c.item_id == free.item_id));
    assert!(claimed.iter().all(|c| c.item_id != locked.item_id));
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn claim_due_items_names_lowest_twin_as_canonical_and_source_name() -> anyhow::Result<()> {
    let Some(fx) = fixture(true).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let first = fx.checkout_file(project, "rules", "style.md", "Keep files short.").await?;
    let second = fx.checkout_file(project, "rules", "style.md", "Keep files short.").await?;
    let other = fx.checkout_file(project, "rules", "other.md", "Something else.").await?;
    for carrier in [&first, &second, &other] {
        fx.embed(carrier, "file-hash", "Keep files short.").await?;
    }

    let claimed = fx.handle.knowledge().claim_due_knowledge_items(10).await?;
    let find = |id: i64| claimed.iter().find(|c| c.item_id == id);
    let canonical = first.item_id.min(second.item_id);
    assert_eq!(find(first.item_id).map(|c| c.canonical_item_id), Some(canonical));
    assert_eq!(find(second.item_id).map(|c| c.canonical_item_id), Some(canonical));
    assert_eq!(find(other.item_id).map(|c| c.canonical_item_id), Some(other.item_id));
    assert_eq!(
        find(first.item_id).and_then(|c| c.source_name.as_deref()),
        Some("rules")
    );
    assert_eq!(
        find(first.item_id).and_then(|c| c.body.as_deref()),
        Some("Keep files short.")
    );

    let carriers = fx
        .handle
        .knowledge()
        .twin_carriers(&[first.item_id, other.item_id])
        .await?;
    let twins_of_first: Vec<i64> = carriers
        .iter()
        .filter(|c| c.item_id == first.item_id)
        .map(|c| c.twin_item_id)
        .collect();
    assert_eq!(twins_of_first.len(), 2);
    assert!(twins_of_first.contains(&second.item_id));
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn deleting_canonical_twin_queues_surviving_twin_that_has_no_statements() -> anyhow::Result<()> {
    let Some(fx) = fixture(true).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let first = fx.checkout_file(project, "rules", "style.md", "Keep files short.").await?;
    let second = fx.checkout_file(project, "rules", "style.md", "Keep files short.").await?;
    let (canonical, heir) = if first.item_id < second.item_id {
        (&first, &second)
    } else {
        (&second, &first)
    };
    fx.embed(heir, "h", "Keep files short.").await?;
    let queued = fx.consolidate_at(heir.item_id).await?;
    if let Some(at) = queued {
        fx.handle
            .knowledge()
            .mark_knowledge_item_consolidated(heir.item_id, at)
            .await?;
    }
    fx.statement_ids(canonical.item_id, "h", vec![statement("Keep files short.", &vector(&[(1, 1.0)]))])
        .await?;
    assert_eq!(fx.consolidate_at(heir.item_id).await?, None);

    sqlx::query("DELETE FROM memory.knowledge_items WHERE id = $1")
        .bind(canonical.item_id)
        .execute(fx.handle.pool())
        .await?;
    assert!(fx.consolidate_at(heir.item_id).await?.is_some());
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn share_twin_embeddings_queues_served_item() -> anyhow::Result<()> {
    let Some(fx) = fixture(true).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let first = fx.checkout_file(project, "rules", "style.md", "Keep files short.").await?;
    let second = fx.checkout_file(project, "rules", "style.md", "Keep files short.").await?;
    fx.embed(&first, "h", "Keep files short.").await?;
    sqlx::query("UPDATE memory.knowledge_items SET needs_embedding = true WHERE id = $1")
        .bind(second.item_id)
        .execute(fx.handle.pool())
        .await?;

    let served = fx
        .handle
        .knowledge()
        .share_twin_knowledge_embeddings(&[second.item_id], &fx.model)
        .await?;
    assert_eq!(served, vec![second.item_id]);
    assert!(fx.consolidate_at(second.item_id).await?.is_some());
    fx.db.cleanup().await?;
    Ok(())
}
