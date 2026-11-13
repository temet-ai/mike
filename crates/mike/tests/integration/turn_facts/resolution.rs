use chrono::{Duration, Utc};
use mike::dossier::build::{BuildConfig, BuildReport, build_sessions};
use mike::store::persistence::dossier_repo::DossierPersist as _;
use mike::store::persistence::turn_facts::TurnFactsRow;

use super::{
    Fixture, NewTurn, Req, attach, fixture, insert_request, insert_text_block, insert_turn,
};

const PEER: &str = "<teammate-message teammate_id=\"peer\">status update please</teammate-message>";

pub(crate) fn cfg() -> BuildConfig {
    BuildConfig {
        settle: Duration::minutes(30),
    }
}

pub(crate) async fn build(f: &Fixture) -> anyhow::Result<BuildReport> {
    Ok(build_sessions(&f.handle, &[f.session_id], Utc::now(), &cfg()).await?)
}

pub(crate) async fn facts(f: &Fixture) -> anyhow::Result<Vec<TurnFactsRow>> {
    Ok(f.handle
        .dossier()
        .load_existing_turn_facts(&[f.session_id])
        .await?)
}

pub(crate) async fn message(
    f: &Fixture,
    id: i64,
    at_secs: i64,
    text: &str,
    closed_secs: Option<i64>,
) -> anyhow::Result<i64> {
    insert_request(f, &Req::primary(id, at_secs, text)).await?;
    let turn = insert_turn(
        f,
        &NewTurn {
            closed_secs,
            ..NewTurn::closed(id, id, at_secs, 0)
        },
    )
    .await?;
    attach(f, turn, &[id]).await?;
    sqlx::query("UPDATE memory.operator_turns SET user_prompt = $2 WHERE id = $1")
        .bind(turn)
        .bind(text)
        .execute(f.handle.pool())
        .await?;
    Ok(turn)
}

fn row_of(rows: &[TurnFactsRow], turn: i64) -> anyhow::Result<&TurnFactsRow> {
    rows.iter()
        .find(|row| row.operator_turn_id == turn)
        .ok_or_else(|| anyhow::anyhow!("no facts row for turn {turn}"))
}

#[tokio::test]
async fn build_sessions_writes_next_genuine_skipping_loop_and_peer() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let first = message(
        &f,
        9100,
        0,
        "please add a retry to the http client",
        Some(100),
    )
    .await?;
    let peer = message(&f, 9101, 100, PEER, Some(200)).await?;
    let last = message(&f, 9102, 200, "thanks, now add the docs", Some(300)).await?;
    let report = build(&f).await?;
    assert_eq!(report.inserted, 3);
    let rows = facts(&f).await?;
    let opener = row_of(&rows, first)?;
    assert_eq!(opener.prompt_kind, "genuine");
    assert_eq!(opener.next_state, "resolved");
    assert_eq!(opener.next_genuine_request_id, Some(9102));
    assert_eq!(opener.next_genuine_turn_id, Some(last));
    assert_eq!(opener.skipped_before_next["peer-message"], 1);
    assert_eq!(opener.reply_window_segment_ids, vec![peer]);
    let segment = row_of(&rows, peer)?;
    assert_eq!(segment.prompt_kind, "peer-message");
    assert_eq!(segment.prompt_rule.as_deref(), Some("a"));
    assert_eq!(row_of(&rows, last)?.next_state, "pending");
    let opened_by: Vec<(i64, Option<String>)> =
        sqlx::query_as("SELECT id, opened_by FROM memory.operator_turns ORDER BY id")
            .fetch_all(f.handle.pool())
            .await?;
    assert_eq!(opened_by[0].1.as_deref(), Some("operator"));
    assert_eq!(opened_by[1].1.as_deref(), Some("machine"));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn orphan_request_can_be_next_genuine() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let first = message(
        &f,
        9100,
        0,
        "please add a retry to the http client",
        Some(100),
    )
    .await?;
    insert_request(&f, &Req::primary(9200, 150, "an orphan follow up message")).await?;
    build(&f).await?;
    let rows = facts(&f).await?;
    let opener = row_of(&rows, first)?;
    assert_eq!(opener.next_genuine_request_id, Some(9200));
    assert_eq!(opener.next_genuine_turn_id, None);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn second_build_with_no_change_writes_nothing() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    message(
        &f,
        9100,
        0,
        "please add a retry to the http client",
        Some(100),
    )
    .await?;
    message(&f, 9101, 100, "and then add the docs", Some(200)).await?;
    let first = build(&f).await?;
    assert_eq!(first.inserted, 2);
    let second = build(&f).await?;
    assert_eq!(
        (second.inserted, second.updated, second.reclassified),
        (0, 0, 0)
    );
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn third_repeat_sets_reclassified_at_on_earlier_turns() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let text =
        |n: u32| format!("Custom poll ping number {n} for the coordinator seat, keep polling");
    let first = message(&f, 9100, 0, &text(7), Some(100)).await?;
    message(&f, 9101, 200, &text(8), Some(300)).await?;
    build(&f).await?;
    let before = facts(&f).await?;
    assert_eq!(row_of(&before, first)?.prompt_kind, "genuine");
    message(&f, 9102, 400, &text(9), Some(500)).await?;
    let report = build(&f).await?;
    assert!(report.reclassified >= 1, "report: {report:?}");
    let after = facts(&f).await?;
    let opener = row_of(&after, first)?;
    assert_eq!(opener.prompt_kind, "loop-wakeup");
    assert_eq!(opener.prompt_rule.as_deref(), Some("b"));
    assert_eq!(opener.prompt_kind_at_close, "genuine");
    assert!(opener.reclassified_at.is_some());
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn settled_session_without_a_reply_is_none_session_settled() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let only = message(
        &f,
        9100,
        0,
        "please add a retry to the http client",
        Some(100),
    )
    .await?;
    sqlx::query("UPDATE sessions.sessions SET last_seen = $2 WHERE id = $1")
        .bind(f.session_id)
        .bind(f.base + Duration::hours(1))
        .execute(f.handle.pool())
        .await?;
    build(&f).await?;
    let rows = facts(&f).await?;
    let row = row_of(&rows, only)?;
    assert_eq!(row.next_state, "none_session_settled");
    assert_eq!(row.next_state_at_close, "pending");
    f.db.cleanup().await?;
    Ok(())
}

async fn turn_with_stale_final_text(f: &Fixture) -> anyhow::Result<i64> {
    let turn = message(
        f,
        9100,
        0,
        "please add a retry to the http client",
        Some(100),
    )
    .await?;
    insert_text_block(f, 9100, 0, "assistant text").await?;
    sqlx::query("UPDATE memory.operator_turns SET final_assistant_text = 'short' WHERE id = $1")
        .bind(turn)
        .execute(f.handle.pool())
        .await?;
    Ok(turn)
}

async fn final_text_of(f: &Fixture, turn: i64) -> anyhow::Result<Option<String>> {
    Ok(
        sqlx::query_scalar("SELECT final_assistant_text FROM memory.operator_turns WHERE id = $1")
            .bind(turn)
            .fetch_one(f.handle.pool())
            .await?,
    )
}

#[tokio::test]
async fn settled_turn_with_stale_final_text_is_rebuilt_then_built() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = turn_with_stale_final_text(&f).await?;
    let report = build(&f).await?;
    assert_eq!(report.skipped_stale, 0);
    assert_eq!(report.inserted, 1);
    assert_eq!(
        final_text_of(&f, turn).await?.as_deref(),
        Some("assistant text")
    );
    assert_eq!(facts(&f).await?.len(), 1);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn unsettled_turn_with_stale_final_text_is_skipped_and_left_alone() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = turn_with_stale_final_text(&f).await?;
    let now = f.base + Duration::minutes(10);
    let report = build_sessions(&f.handle, &[f.session_id], now, &cfg()).await?;
    assert_eq!(report.skipped_stale, 1);
    assert_eq!(report.inserted, 0);
    assert_eq!(final_text_of(&f, turn).await?.as_deref(), Some("short"));
    assert!(facts(&f).await?.is_empty());
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn build_sessions_on_missing_session_is_ok_and_writes_nothing() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let report = build_sessions(&f.handle, &[9_999_999], Utc::now(), &cfg()).await?;
    assert_eq!(report, BuildReport::default());
    build_sessions(&f.handle, &[], Utc::now(), &cfg()).await?;
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn session_class_follows_the_launch_row() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = message(
        &f,
        9100,
        0,
        "please add a retry to the http client",
        Some(100),
    )
    .await?;
    build(&f).await?;
    assert_eq!(row_of(&facts(&f).await?, turn)?.session_class, "operator");
    sqlx::query(
        "INSERT INTO launch.launches (session_uuid, backend, folder, seat)
         SELECT session_uuid, 'herdr', '/work', 'jc02-seat' FROM sessions.sessions WHERE id = $1",
    )
    .bind(f.session_id)
    .execute(f.handle.pool())
    .await?;
    let briefed_only = build(&f).await?;
    assert_eq!(briefed_only.updated, 0);
    assert_eq!(row_of(&facts(&f).await?, turn)?.session_class, "operator");
    let run = format!("fleet-jc02-{}", f.session_id);
    sqlx::query("INSERT INTO launch.runs (run_id, backend) VALUES ($1, 'herdr')")
        .bind(&run)
        .execute(f.handle.pool())
        .await?;
    sqlx::query(
        "UPDATE launch.launches SET run_id = $2
          WHERE session_uuid = (SELECT session_uuid FROM sessions.sessions WHERE id = $1)",
    )
    .bind(f.session_id)
    .bind(&run)
    .execute(f.handle.pool())
    .await?;
    let report = build(&f).await?;
    assert_eq!(report.updated, 1);
    let row = facts(&f).await?;
    assert_eq!(row_of(&row, turn)?.session_class, "fleet_seat");
    assert_eq!(row_of(&row, turn)?.session_class_rule, "launch");
    f.db.cleanup().await?;
    Ok(())
}
