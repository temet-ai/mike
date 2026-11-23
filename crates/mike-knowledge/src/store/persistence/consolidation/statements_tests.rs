use super::test_fixture::{fixture, statement, statements_of, vector};
use super::{NewStatement, StatementStateChange};
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use chrono::{Duration, Utc};

#[tokio::test]
async fn replace_item_statements_inserts_numbered_rows_and_returns_ids() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let file = fx.checkout_file(project, "rules", "a.md", "One. Two.").await?;
    let knowledge = fx.handle.knowledge();

    let replaced = knowledge
        .replace_item_statements(
            file.item_id,
            &statements_of(
                "h1",
                vec![
                    NewStatement {
                        passage_start: Some(0),
                        passage_end: Some(4),
                        ..statement("One.", &vector(&[(1, 1.0)]))
                    },
                    statement("Two.", &vector(&[(2, 1.0)])),
                ],
            ),
        )
        .await?;
    assert_eq!(replaced.created.len(), 2);
    assert_eq!(replaced.created[0].ordinal, 0);
    assert_eq!(replaced.created[1].ordinal, 1);
    assert!(replaced.gone.is_empty());

    let stored: Vec<(String, Option<i32>, String)> = sqlx::query_as(
        "SELECT text, passage_end, split_hash FROM memory.knowledge_statements
          WHERE item_id = $1 ORDER BY ordinal",
    )
    .bind(file.item_id)
    .fetch_all(fx.handle.pool())
    .await?;
    assert_eq!(stored[0], ("One.".into(), Some(4), "h1".into()));
    assert_eq!(stored[1], ("Two.".into(), None, "h1".into()));
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn replace_item_statements_marks_previous_gone_and_continues_ordinals() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let file = fx.checkout_file(project, "rules", "a.md", "One.").await?;
    let first = fx
        .statement_ids(file.item_id, "h1", vec![statement("One.", &vector(&[(1, 1.0)]))])
        .await?;
    let subject = fx
        .handle
        .knowledge()
        .create_knowledge_subject(Some(project), first[0], Utc::now() - Duration::seconds(1))
        .await?;

    let replaced = fx
        .handle
        .knowledge()
        .replace_item_statements(
            file.item_id,
            &statements_of("h2", vec![statement("One, rewritten.", &vector(&[(1, 1.0)]))]),
        )
        .await?;
    assert_eq!(replaced.created[0].ordinal, 1);
    assert_eq!(replaced.gone.len(), 1);
    assert_eq!(replaced.gone[0].id, first[0]);
    assert_eq!(replaced.gone[0].subject_id, Some(subject));

    let (state, members): (String, i32) = sqlx::query_as(
        "SELECT s.state, k.members FROM memory.knowledge_statements s
           JOIN memory.knowledge_subjects k ON k.id = s.subject_id WHERE s.id = $1",
    )
    .bind(first[0])
    .fetch_one(fx.handle.pool())
    .await?;
    assert_eq!(state, "gone");
    assert_eq!(members, 0);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn replace_item_statements_marks_twin_statements_gone() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let first = fx.checkout_file(project, "rules", "a.md", "One.").await?;
    let second = fx.checkout_file(project, "rules", "a.md", "One.").await?;
    let (canonical, other) = if first.item_id < second.item_id {
        (&first, &second)
    } else {
        (&second, &first)
    };
    let stray = fx
        .statement_ids(other.item_id, "h", vec![statement("One.", &vector(&[(1, 1.0)]))])
        .await?;

    let replaced = fx
        .handle
        .knowledge()
        .replace_item_statements(
            canonical.item_id,
            &statements_of("h", vec![statement("One.", &vector(&[(1, 1.0)]))]),
        )
        .await?;
    assert_eq!(replaced.gone.iter().map(|g| g.id).collect::<Vec<_>>(), stray);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn replace_item_statements_with_bad_passage_fails_and_changes_nothing() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let file = fx.checkout_file(project, "rules", "a.md", "One.").await?;
    let kept = fx
        .statement_ids(file.item_id, "h1", vec![statement("One.", &vector(&[(1, 1.0)]))])
        .await?;

    let bad = NewStatement {
        passage_start: Some(10),
        passage_end: Some(2),
        ..statement("Broken.", &vector(&[(2, 1.0)]))
    };
    let result = fx
        .handle
        .knowledge()
        .replace_item_statements(file.item_id, &statements_of("h2", vec![bad]))
        .await;
    assert!(result.is_err());

    let state: String =
        sqlx::query_scalar("SELECT state FROM memory.knowledge_statements WHERE id = $1")
            .bind(kept[0])
            .fetch_one(fx.handle.pool())
            .await?;
    assert_eq!(state, "current");
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn mark_item_statements_gone_leaves_subject_without_the_statements() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let lesson = fx.lesson(project, "Be brief.").await?;
    let ids = fx
        .statement_ids(lesson.item_id, "h", vec![statement("Be brief.", &vector(&[(3, 1.0)]))])
        .await?;
    let knowledge = fx.handle.knowledge();
    let subject = knowledge
        .create_knowledge_subject(Some(project), ids[0], Utc::now())
        .await?;

    let gone = knowledge
        .mark_item_statements_gone(lesson.item_id, Utc::now())
        .await?;
    assert_eq!(gone.len(), 1);
    assert!(knowledge.knowledge_subject_statements(subject).await?.is_empty());
    assert!(knowledge.mark_item_statements_gone(lesson.item_id, Utc::now()).await?.is_empty());
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn deleting_item_removes_statements_and_marks_subject_changed() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let keep = fx.lesson(project, "Keep me.").await?;
    let drop = fx.lesson(project, "Drop me.").await?;
    let keep_ids = fx
        .statement_ids(keep.item_id, "h", vec![statement("Keep me.", &vector(&[(4, 1.0)]))])
        .await?;
    let drop_ids = fx
        .statement_ids(drop.item_id, "h", vec![statement("Drop me.", &vector(&[(4, 0.9), (5, 0.1)]))])
        .await?;
    let knowledge = fx.handle.knowledge();
    let subject = knowledge
        .create_knowledge_subject(Some(project), keep_ids[0], Utc::now())
        .await?;
    knowledge
        .add_statement_to_subject(drop_ids[0], subject, Utc::now())
        .await?;
    let claimed_at = sqlx::query_scalar::<_, chrono::DateTime<Utc>>(
        "SELECT changed_at FROM memory.knowledge_subjects WHERE id = $1",
    )
    .bind(subject)
    .fetch_one(fx.handle.pool())
    .await?;
    assert!(knowledge.finish_knowledge_subject_judgement(subject, claimed_at, None).await?);

    sqlx::query("DELETE FROM memory.knowledge_items WHERE id = $1")
        .bind(drop.item_id)
        .execute(fx.handle.pool())
        .await?;

    let (members, changed): (i32, bool) = sqlx::query_as(
        "SELECT members, changed_at IS NOT NULL FROM memory.knowledge_subjects WHERE id = $1",
    )
    .bind(subject)
    .fetch_one(fx.handle.pool())
    .await?;
    assert_eq!(members, 1);
    assert!(changed);
    let remaining: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM memory.knowledge_statements WHERE item_id = $1",
    )
    .bind(drop.item_id)
    .fetch_one(fx.handle.pool())
    .await?;
    assert_eq!(remaining, 0);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn nearest_statements_stays_in_scope_skips_resolved_and_orders_by_similarity()
-> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let (mine, theirs) = (fx.project().await?, fx.project().await?);
    let near = fx.lesson(mine, "Near.").await?;
    let far = fx.lesson(mine, "Far.").await?;
    let covered = fx.lesson(mine, "Covered.").await?;
    let foreign = fx.lesson(theirs, "Foreign.").await?;
    let near_id = fx.statement_ids(near.item_id, "h", vec![statement("Near.", &vector(&[(10, 1.0), (11, 0.1)]))]).await?[0];
    let far_id = fx.statement_ids(far.item_id, "h", vec![statement("Far.", &vector(&[(10, 0.3), (12, 1.0)]))]).await?[0];
    let covered_id = fx.statement_ids(covered.item_id, "h", vec![statement("Covered.", &vector(&[(10, 1.0)]))]).await?[0];
    let foreign_id = fx.statement_ids(foreign.item_id, "h", vec![statement("Foreign.", &vector(&[(10, 1.0)]))]).await?[0];

    let knowledge = fx.handle.knowledge();
    knowledge
        .set_statement_states(&[StatementStateChange {
            statement_id: covered_id,
            state: "covered".into(),
            resolved_by_statement_id: Some(near_id),
        }])
        .await?;

    let found = knowledge
        .nearest_statements(Some(mine), &vector(&[(10, 1.0)]), &[], 50)
        .await?;
    let ids: Vec<i64> = found.iter().map(|n| n.statement_id).collect();
    assert!(ids.contains(&near_id) && ids.contains(&far_id));
    assert!(!ids.contains(&covered_id) && !ids.contains(&foreign_id));
    let position = |id: i64| ids.iter().position(|x| *x == id);
    assert!(position(near_id) < position(far_id));
    assert!(found.iter().find(|n| n.statement_id == near_id).is_some_and(|n| n.similarity > 0.99));

    let excluded = knowledge
        .nearest_statements(Some(mine), &vector(&[(10, 1.0)]), &[near_id], 50)
        .await?;
    assert!(excluded.iter().all(|n| n.statement_id != near_id));
    let limited = knowledge
        .nearest_statements(Some(mine), &vector(&[(10, 1.0)]), &[], 1)
        .await?;
    assert_eq!(limited.len(), 1);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn set_statement_states_records_resolver_and_rejects_bad_input() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let (a, b) = (fx.lesson(project, "A.").await?, fx.lesson(project, "B.").await?);
    let a_id = fx.statement_ids(a.item_id, "h", vec![statement("A.", &vector(&[(6, 1.0)]))]).await?[0];
    let b_id = fx.statement_ids(b.item_id, "h", vec![statement("B.", &vector(&[(6, 1.0)]))]).await?[0];
    let knowledge = fx.handle.knowledge();

    let touched = knowledge
        .set_statement_states(&[StatementStateChange {
            statement_id: b_id,
            state: "superseded".into(),
            resolved_by_statement_id: Some(a_id),
        }])
        .await?;
    assert_eq!(touched, 1);
    let resolved: (String, Option<i64>) = sqlx::query_as(
        "SELECT state, resolved_by_statement_id FROM memory.knowledge_statements WHERE id = $1",
    )
    .bind(b_id)
    .fetch_one(fx.handle.pool())
    .await?;
    assert_eq!(resolved, ("superseded".into(), Some(a_id)));

    let unknown_state = knowledge
        .set_statement_states(&[StatementStateChange {
            statement_id: a_id,
            state: "vanished".into(),
            resolved_by_statement_id: None,
        }])
        .await;
    assert!(unknown_state.is_err());
    let resolver_on_current = knowledge
        .set_statement_states(&[StatementStateChange {
            statement_id: a_id,
            state: "current".into(),
            resolved_by_statement_id: Some(b_id),
        }])
        .await;
    assert!(resolver_on_current.is_err());
    assert_eq!(knowledge.set_statement_states(&[]).await?, 0);
    fx.db.cleanup().await?;
    Ok(())
}
