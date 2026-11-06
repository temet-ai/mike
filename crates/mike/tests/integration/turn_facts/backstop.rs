use chrono::{Duration, Utc};
use mike::store::persistence::prompt_classification::ClassificationWrite;
use mike::store::persistence::turn_facts::CandidateWindow;

use super::{Fixture, NewTurn, Req, attach, fixture, insert_request, insert_turn, sample_row};

const VERSION: &str = "turn-facts-test";

fn window(f: &Fixture) -> CandidateWindow {
    CandidateWindow {
        lookback_from: f.base - Duration::days(1),
        settled_before: Utc::now() - Duration::minutes(30),
        limit: 25,
    }
}

async fn seed_turn_with_facts(f: &Fixture) -> anyhow::Result<i64> {
    insert_request(f, &Req::primary(9100, 0, "ask")).await?;
    let turn = insert_turn(f, &NewTurn::closed(9100, 9100, 0, 50)).await?;
    attach(f, turn, &[9100]).await?;
    let mut row = sample_row(turn, f.session_id, f.base + Duration::seconds(60));
    row.max_request_id = 9100;
    f.handle.upsert_turn_facts(&[row]).await?;
    Ok(turn)
}

#[tokio::test]
async fn backstop_finds_idle_reaped_turn_without_facts() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    insert_request(&f, &Req::primary(9100, 0, "ask")).await?;
    let turn = insert_turn(&f, &NewTurn::closed(9100, 9100, 0, 50)).await?;
    attach(&f, turn, &[9100]).await?;
    let found = f
        .handle
        .find_turn_facts_candidates(VERSION, window(&f))
        .await?;
    assert_eq!(found, vec![f.session_id]);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn backstop_ignores_a_fully_built_session() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    seed_turn_with_facts(&f).await?;
    let found = f
        .handle
        .find_turn_facts_candidates(VERSION, window(&f))
        .await?;
    assert!(found.is_empty(), "unexpected candidates: {found:?}");
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn backstop_finds_pending_facts_after_new_message() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    seed_turn_with_facts(&f).await?;
    insert_request(&f, &Req::primary(9200, 300, "a later message")).await?;
    f.handle
        .write_classification_row(&ClassificationWrite {
            request_id: 9200,
            session_db_id: f.session_id,
            kind_code: "operator",
            rule: None,
            repeat_key: None,
            classifier_version: "genuine-v1",
        })
        .await?;
    let found = f
        .handle
        .find_turn_facts_candidates(VERSION, window(&f))
        .await?;
    assert_eq!(found, vec![f.session_id]);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn backstop_finds_pending_facts_once_the_session_has_settled() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    seed_turn_with_facts(&f).await?;
    sqlx::query("UPDATE sessions.sessions SET last_seen = $2 WHERE id = $1")
        .bind(f.session_id)
        .bind(Utc::now() - Duration::hours(1))
        .execute(f.handle.pool())
        .await?;
    let found = f
        .handle
        .find_turn_facts_candidates(VERSION, window(&f))
        .await?;
    assert_eq!(found, vec![f.session_id]);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn backstop_finds_builder_version_change() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    seed_turn_with_facts(&f).await?;
    let found = f
        .handle
        .find_turn_facts_candidates("turn-facts-next", window(&f))
        .await?;
    assert_eq!(found, vec![f.session_id]);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn backstop_finds_request_attached_after_build() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = seed_turn_with_facts(&f).await?;
    insert_request(&f, &Req::lane(9105, "primary", 70)).await?;
    attach(&f, turn, &[9105]).await?;
    let found = f
        .handle
        .find_turn_facts_candidates(VERSION, window(&f))
        .await?;
    assert_eq!(found, vec![f.session_id]);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn backstop_ignores_turns_older_than_lookback() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    insert_request(&f, &Req::primary(9100, 0, "ask")).await?;
    let turn = insert_turn(&f, &NewTurn::closed(9100, 9100, 0, 50)).await?;
    attach(&f, turn, &[9100]).await?;
    let narrow = CandidateWindow {
        lookback_from: Utc::now(),
        ..window(&f)
    };
    let found = f.handle.find_turn_facts_candidates(VERSION, narrow).await?;
    assert!(found.is_empty());
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn backstop_ignores_open_turns() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    insert_request(&f, &Req::primary(9100, 0, "ask")).await?;
    let open = NewTurn {
        closed_secs: None,
        ..NewTurn::closed(9100, 9100, 0, 0)
    };
    insert_turn(&f, &open).await?;
    let found = f
        .handle
        .find_turn_facts_candidates(VERSION, window(&f))
        .await?;
    assert!(found.is_empty());
    f.db.cleanup().await?;
    Ok(())
}
