use chrono::Duration;
use mike::store::persistence::turn_facts::OpenedByRow;

use super::{Fixture, NewTurn, Req, attach, fixture, insert_request, insert_turn, sample_row};

async fn seed_turn(f: &Fixture) -> anyhow::Result<i64> {
    insert_request(f, &Req::primary(9100, 0, "ask")).await?;
    let turn = insert_turn(f, &NewTurn::closed(9100, 9100, 0, 50)).await?;
    attach(f, turn, &[9100]).await?;
    Ok(turn)
}

#[tokio::test]
async fn upsert_inserts_then_identical_rebuild_writes_nothing() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = seed_turn(&f).await?;
    let first = sample_row(turn, f.session_id, f.base);
    let outcome = f
        .handle
        .upsert_turn_facts(std::slice::from_ref(&first))
        .await?;
    assert_eq!((outcome.inserted, outcome.updated), (1, 0));
    let later = sample_row(turn, f.session_id, f.base + Duration::minutes(5));
    let again = f.handle.upsert_turn_facts(&[later]).await?;
    assert_eq!(
        (again.inserted, again.updated, again.reclassified),
        (0, 0, 0)
    );
    let stored = f.handle.load_existing_turn_facts(&[f.session_id]).await?;
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0], first);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn at_close_columns_are_not_overwritten_by_later_builds() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = seed_turn(&f).await?;
    f.handle
        .upsert_turn_facts(&[sample_row(turn, f.session_id, f.base)])
        .await?;
    let mut changed = sample_row(turn, f.session_id, f.base + Duration::minutes(5));
    changed.prompt_kind_at_close = "loop-wakeup".to_string();
    changed.next_state_at_close = "resolved".to_string();
    changed.next_genuine_request_id_at_close = Some(9200);
    changed.text_chars = 10;
    changed.text_cut_at = 4;
    let outcome = f.handle.upsert_turn_facts(&[changed]).await?;
    assert_eq!(outcome.updated, 1);
    let stored = f.handle.load_existing_turn_facts(&[f.session_id]).await?;
    assert_eq!(stored[0].prompt_kind_at_close, "genuine");
    assert_eq!(stored[0].next_state_at_close, "pending");
    assert_eq!(stored[0].next_genuine_request_id_at_close, None);
    assert_eq!(stored[0].text_chars, 10);
    assert_eq!(stored[0].built_at, f.base + Duration::minutes(5));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn prompt_kind_change_sets_reclassified_at() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = seed_turn(&f).await?;
    f.handle
        .upsert_turn_facts(&[sample_row(turn, f.session_id, f.base)])
        .await?;
    let mut looped = sample_row(turn, f.session_id, f.base + Duration::minutes(5));
    looped.prompt_kind = "loop-wakeup".to_string();
    looped.prompt_rule = Some("b".to_string());
    let outcome = f.handle.upsert_turn_facts(&[looped]).await?;
    assert_eq!(outcome.reclassified, 1);
    let stored = f.handle.load_existing_turn_facts(&[f.session_id]).await?;
    assert!(stored[0].reclassified_at.is_some());
    assert_eq!(stored[0].prompt_kind_at_close, "genuine");
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn pending_to_resolved_is_not_a_reclassification() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = seed_turn(&f).await?;
    f.handle
        .upsert_turn_facts(&[sample_row(turn, f.session_id, f.base)])
        .await?;
    let mut resolved = sample_row(turn, f.session_id, f.base + Duration::minutes(5));
    resolved.next_state = "resolved".to_string();
    resolved.next_genuine_request_id = Some(9200);
    let outcome = f.handle.upsert_turn_facts(&[resolved]).await?;
    assert_eq!((outcome.updated, outcome.reclassified), (1, 0));
    let stored = f.handle.load_existing_turn_facts(&[f.session_id]).await?;
    assert_eq!(stored[0].reclassified_at, None);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn changed_next_genuine_request_is_a_reclassification() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = seed_turn(&f).await?;
    let mut first = sample_row(turn, f.session_id, f.base);
    first.next_state = "resolved".to_string();
    first.next_genuine_request_id = Some(9200);
    f.handle.upsert_turn_facts(&[first]).await?;
    let mut second = sample_row(turn, f.session_id, f.base + Duration::minutes(5));
    second.next_state = "resolved".to_string();
    second.next_genuine_request_id = Some(9300);
    let outcome = f.handle.upsert_turn_facts(&[second]).await?;
    assert_eq!(outcome.reclassified, 1);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn upsert_with_invalid_prompt_kind_is_err() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = seed_turn(&f).await?;
    let mut bad = sample_row(turn, f.session_id, f.base);
    bad.prompt_kind = "not-a-kind".to_string();
    assert!(f.handle.upsert_turn_facts(&[bad]).await.is_err());
    assert!(
        f.handle
            .load_existing_turn_facts(&[f.session_id])
            .await?
            .is_empty()
    );
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn resolved_state_without_next_request_is_err() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = seed_turn(&f).await?;
    let mut bad = sample_row(turn, f.session_id, f.base);
    bad.next_state = "resolved".to_string();
    assert!(f.handle.upsert_turn_facts(&[bad]).await.is_err());
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn upsert_of_no_rows_is_a_no_op() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let outcome = f.handle.upsert_turn_facts(&[]).await?;
    assert_eq!((outcome.inserted, outcome.updated), (0, 0));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn fill_opened_by_updates_only_null_rows() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let unset = seed_turn(&f).await?;
    insert_request(&f, &Req::primary(9200, 100, "later")).await?;
    let set = insert_turn(&f, &NewTurn::closed(9200, 9200, 100, 150)).await?;
    sqlx::query("UPDATE memory.operator_turns SET opened_by = 'operator' WHERE id = $1")
        .bind(set)
        .execute(f.handle.pool())
        .await?;
    let written = f
        .handle
        .fill_operator_turn_opened_by(&[
            OpenedByRow {
                turn_id: unset,
                opened_by: "machine".to_string(),
            },
            OpenedByRow {
                turn_id: set,
                opened_by: "agent".to_string(),
            },
        ])
        .await?;
    assert_eq!(written, 1);
    let stored: Vec<(i64, Option<String>)> =
        sqlx::query_as("SELECT id, opened_by FROM memory.operator_turns ORDER BY id")
            .fetch_all(f.handle.pool())
            .await?;
    assert_eq!(stored[0].1.as_deref(), Some("machine"));
    assert_eq!(stored[1].1.as_deref(), Some("operator"));
    assert_eq!(f.handle.fill_operator_turn_opened_by(&[]).await?, 0);
    f.db.cleanup().await?;
    Ok(())
}
