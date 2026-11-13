use crate::store::persistence::steerer_repo::SteererPersist as _;
use chrono::{DateTime, Duration, Utc};

use super::ReplayItem;
use super::as_of::assemble;
use super::parity::{ParityStatus, RecordedCall, parity_recorded};
use super::run::load_replay_steerer;
use super::tests_db::{Seeded, seed};
use crate::steerer::fleet::input_budget::VerifierInputBudget;
use crate::store::persistence::test_support;

const VISIBLE: [&str; 4] = ["t_u1", "t_u3", "t_u6", "t_u7"];
const HIDDEN: [&str; 4] = ["t_u2", "t_u4", "t_u5", "t_u8"];

struct Timeline {
    completed: DateTime<Utc>,
    second_request: i64,
}

async fn insert_use(
    s: &Seeded,
    request_id: i64,
    ord: i32,
    name: &str,
    created_at: DateTime<Utc>,
    block_arrived_at: Option<DateTime<Utc>>,
) -> anyhow::Result<()> {
    let wire = format!("wire_{name}");
    sqlx::query!(
        "INSERT INTO tools.uses
            (wire_tool_use_id, request_id, session_id, ord, tool_name, input, operation, created_at)
         VALUES ($1, $2, $3, $4, $5, '{}'::jsonb, 'exec', $6)",
        wire,
        request_id,
        s.session_db_id,
        ord,
        name,
        created_at,
    )
    .execute(s.handle.pool())
    .await?;
    if let Some(arrived_at) = block_arrived_at {
        insert_block(s, request_id, ord, &wire, arrived_at).await?;
    }
    Ok(())
}

async fn insert_block(
    s: &Seeded,
    request_id: i64,
    ord: i32,
    wire: &str,
    arrived_at: DateTime<Utc>,
) -> anyhow::Result<()> {
    sqlx::query!(
        "INSERT INTO responses.stream_blocks
                (request_id, session_id, block_type_id, seq_ord, content, content_sha256,
                 wire_tool_use_id, arrived_at, byte_size)
             VALUES ($1, $2, 3, $3, '', $4, $5, $6, 0)",
        request_id,
        s.session_db_id,
        ord,
        "0".repeat(64),
        wire,
        arrived_at,
    )
    .execute(s.handle.pool())
    .await?;
    Ok(())
}

async fn build_timeline(s: &Seeded) -> anyhow::Result<Timeline> {
    let at = |secs: i64| s.base + Duration::seconds(secs);
    let project_id = sqlx::query_scalar!(
        "SELECT project_id FROM requests.requests WHERE id = $1",
        s.request_id,
    )
    .fetch_one(s.handle.pool())
    .await?;
    test_support::seed_session_turns(s.handle.pool(), &s.session_uuid.to_string(), project_id, 1)
        .await?;
    let second_request = sqlx::query_scalar!(
        "SELECT id FROM requests.requests WHERE session_id = $1 AND id <> $2 ORDER BY id LIMIT 1",
        s.session_db_id,
        s.request_id,
    )
    .fetch_one(s.handle.pool())
    .await?;
    stamp_requests(s, second_request).await?;
    insert_timeline_uses(s, second_request).await?;
    Ok(Timeline {
        completed: at(105),
        second_request,
    })
}

async fn stamp_requests(s: &Seeded, second_request: i64) -> anyhow::Result<()> {
    let at = |secs: i64| s.base + Duration::seconds(secs);
    sqlx::query!(
        "UPDATE requests.requests SET created_at = $2, completed_at = $3 WHERE id = $1",
        s.request_id,
        at(100),
        at(105),
    )
    .execute(s.handle.pool())
    .await?;
    sqlx::query!(
        "UPDATE requests.requests
            SET created_at = $2, completed_at = $3, operator_turn_id = $4 WHERE id = $1",
        second_request,
        at(101),
        at(200),
        s.turn_id,
    )
    .execute(s.handle.pool())
    .await?;
    Ok(())
}

async fn insert_timeline_uses(s: &Seeded, second_request: i64) -> anyhow::Result<()> {
    let at = |secs: i64| s.base + Duration::seconds(secs);
    let (r1, r2) = (s.request_id, second_request);
    insert_use(s, r1, 1, "t_u1", at(100), Some(at(102))).await?;
    insert_use(s, r1, 2, "t_u2", at(100), Some(at(110))).await?;
    insert_use(s, r2, 3, "t_u3", at(101), Some(at(103))).await?;
    insert_use(s, r2, 4, "t_u4", at(101), Some(at(150))).await?;
    insert_use(s, r2, 5, "t_u5", at(50), Some(at(120))).await?;
    insert_use(s, r2, 6, "t_u6", at(150), Some(at(104))).await?;
    insert_use(s, r1, 7, "t_u7", at(100), None).await?;
    insert_use(s, r2, 8, "t_u8", at(101), None).await?;
    Ok(())
}

fn sorted(mut names: Vec<String>) -> Vec<String> {
    names.sort();
    names
}

#[tokio::test]
async fn as_of_visibility_follows_stream_block_write_time() -> anyhow::Result<()> {
    let Some(s) = seed().await? else {
        return Ok(());
    };
    let timeline = build_timeline(&s).await?;
    let trigger = s
        .handle
        .steerer()
        .fetch_replay_trigger(s.request_id)
        .await?;
    assert_eq!(trigger.trigger_at, timeline.completed);

    let rows = s
        .handle
        .steerer()
        .fetch_session_action_list_as_of(s.session_db_id, trigger.trigger_at)
        .await?;
    let listed = sorted(rows.iter().map(|r| r.tool_name.clone()).collect());
    let expected = sorted(VISIBLE.iter().map(|n| (*n).to_owned()).collect());
    assert_eq!(listed, expected, "hidden: {HIDDEN:?}");
    assert_eq!(
        rows.iter()
            .map(|r| r.tool_name.as_str())
            .collect::<Vec<_>>(),
        ["t_u1", "t_u7", "t_u3", "t_u6"],
        "live ordering by created_at then ord"
    );

    let recent = s
        .handle
        .steerer()
        .fetch_session_action_list_recent_as_of(s.session_db_id, trigger.trigger_at, 100)
        .await?;
    assert_eq!(recent.len(), 4);
    let counts = s
        .handle
        .steerer()
        .fetch_session_action_counts_as_of(s.session_db_id, trigger.trigger_at)
        .await?;
    assert_eq!(counts.total, 4);

    let trajectory = s
        .handle
        .steerer()
        .fetch_turn_tool_calls_as_of(s.turn_id, s.session_db_id, trigger.trigger_at)
        .await?;
    let ids = sorted(trajectory.iter().map(|c| c.tool_id.clone()).collect());
    let expected_ids = sorted(VISIBLE.iter().map(|n| format!("wire_{n}")).collect());
    assert_eq!(ids, expected_ids);
    assert!(
        trajectory
            .iter()
            .any(|c| c.request_id == timeline.second_request),
        "a tool use from a request after the trigger, written before T, is included"
    );

    s.handle.pool().close().await;
    s.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn two_recorded_calls_on_one_trigger_share_one_as_of() -> anyhow::Result<()> {
    let Some(s) = seed().await? else {
        return Ok(());
    };
    build_timeline(&s).await?;
    let replay = load_replay_steerer(&s.handle).await?;
    let item = ReplayItem {
        turn_id: s.turn_id,
        trigger_request_id: s.request_id,
    };
    let message = assemble(&s.handle, &item, &replay, VerifierInputBudget::Uncapped)
        .await?
        .user_message;
    let call = |call_id: i64, latency_ms: i64| RecordedCall {
        call_id,
        turn_id: Some(s.turn_id),
        trigger_request_id: Some(s.request_id),
        request_body: serde_json::json!({"messages": [{"role": "user", "content": message}]}),
        called_at: Some(s.base + Duration::seconds(300)),
        latency_ms: Some(latency_ms),
    };
    let rows = parity_recorded(&s.handle, &replay, &[call(1, 1_000), call(2, 90_000)], None).await;
    for row in &rows {
        assert_eq!(row.as_of, "trigger_completed_at");
        assert_eq!(row.result, ParityStatus::Equal);
    }
    assert_ne!(rows[0].window_end, rows[1].window_end);

    s.handle.pool().close().await;
    s.db.cleanup().await?;
    Ok(())
}
