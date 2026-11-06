use chrono::{DateTime, Duration, Utc};

use super::ReplayItem;
use super::as_of::assemble;
use super::run::{ItemOutcome, ReplayMode, load_replay_steerer, replay_items};
use crate::dossier::render::steerer::RECENT_REACTIONS_HEADING;
use crate::dossier::render::steerer_notes::{
    EARLIER_NOTES_HEADING, EARLIER_TURNS_HEADING, TURN_OPENING_HEADING,
};
use crate::steerer::fleet::input_budget::VerifierInputBudget;
use crate::steerer::fleet::verifier_judge::build_verifier_judge_context;
use crate::steerer::note_policy::{self, NoteRefusal};
use crate::steerer::verdict::Need;
use crate::store::persistence::PersistHandle;
use crate::store::test_support::{self, TestDb, TestDbHandle};

pub(super) struct Seeded {
    pub(super) db: TestDbHandle,
    pub(super) handle: PersistHandle,
    pub(super) session_uuid: uuid::Uuid,
    pub(super) session_db_id: i64,
    pub(super) request_id: i64,
    pub(super) turn_id: i64,
    pub(super) base: DateTime<Utc>,
}

pub(super) async fn seed() -> anyhow::Result<Option<Seeded>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = test_db.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project_id = test_support::seed_project(handle.pool(), "verifier-replay").await?;
    let session_uuid = uuid::Uuid::new_v4();
    let other_uuid = uuid::Uuid::new_v4();
    test_support::seed_two_sessions(
        handle.pool(),
        &session_uuid.to_string(),
        &other_uuid.to_string(),
        project_id,
    )
    .await?;
    test_support::seed_session_turns(handle.pool(), &session_uuid.to_string(), project_id, 1)
        .await?;
    let ids = sqlx::query!(
        "SELECT s.id AS session_db_id, r.id AS request_id, t.id AS turn_id
           FROM sessions.sessions s
           JOIN requests.requests r ON r.session_id = s.id
           JOIN memory.operator_turns t ON t.start_request_id = r.id
          WHERE s.session_uuid = $1",
        session_uuid,
    )
    .fetch_one(handle.pool())
    .await?;
    let (session_db_id, request_id, turn_id) = (ids.session_db_id, ids.request_id, ids.turn_id);
    sqlx::query!(
        "UPDATE requests.requests SET operator_turn_id = $2 WHERE id = $1",
        request_id,
        turn_id,
    )
    .execute(handle.pool())
    .await?;
    let base =
        DateTime::from_timestamp_micros((Utc::now() - Duration::hours(1)).timestamp_micros())
            .unwrap_or_else(Utc::now);
    Ok(Some(Seeded {
        db,
        handle,
        session_uuid,
        session_db_id,
        request_id,
        turn_id,
        base,
    }))
}

async fn seed_tool_use(s: &Seeded, i: i32) -> anyhow::Result<()> {
    let at = s.base + Duration::seconds(i64::from(i));
    let use_id = insert_seed_use(s, i, at).await?;
    insert_seed_result(s, i, use_id, at).await
}

fn seed_tool_shape(i: i32) -> (&'static str, &'static str, Option<&'static str>) {
    match i % 3 {
        0 => ("Read", "read", None),
        1 => ("Edit", "write", None),
        _ => ("Bash", "exec", Some("make test")),
    }
}

async fn insert_seed_use(s: &Seeded, i: i32, at: DateTime<Utc>) -> anyhow::Result<i64> {
    let (name, op, command) = seed_tool_shape(i);
    let use_id = sqlx::query_scalar!(
        "INSERT INTO tools.uses
                (wire_tool_use_id, request_id, session_id, ord, tool_name, input, operation, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             RETURNING id",
        format!("toolu_replay_{i}"),
        s.request_id,
        s.session_db_id,
        i,
        name,
        command.map(|c| serde_json::json!({ "command": c })),
        op,
        at,
    )
    .fetch_one(s.handle.pool())
    .await?;
    Ok(use_id)
}

async fn insert_seed_result(
    s: &Seeded,
    i: i32,
    use_id: i64,
    at: DateTime<Utc>,
) -> anyhow::Result<()> {
    sqlx::query!(
        "INSERT INTO tools.results
                (request_id, session_id, tool_use_id, ord, is_error, content, content_sha256, content_bytes, created_at)
             VALUES ($1, $2, $3, 0, $4, '\"ok\"'::jsonb, $5, 4, $6)",
        s.request_id,
        s.session_db_id,
        use_id,
        i % 6 == 2,
        "0".repeat(64),
        at + Duration::milliseconds(300),
    )
    .execute(s.handle.pool())
    .await?;
    Ok(())
}

async fn seed_asks(s: &Seeded) -> anyhow::Result<()> {
    for (i, kind) in ["initial", "clarification"].into_iter().enumerate() {
        sqlx::query!(
            "INSERT INTO memory.ask_ledger_entries (session_id, operator_turn_id, kind, content, created_at)
             VALUES ($1, NULL, $2, $3, $4)",
            s.session_db_id,
            kind,
            format!("ask number {i}"),
            s.base + Duration::seconds(i as i64),
        )
        .execute(s.handle.pool())
        .await?;
    }
    Ok(())
}

pub(super) async fn seed_activity(s: &Seeded) -> anyhow::Result<()> {
    for i in 0..30i32 {
        seed_tool_use(s, i).await?;
    }
    seed_asks(s).await?;
    sqlx::query!(
        "UPDATE requests.requests SET created_at = $2 WHERE id = $1",
        s.request_id,
        s.base + Duration::seconds(100),
    )
    .execute(s.handle.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn replay_rebuild_matches_live_builder_on_seeded_session() -> anyhow::Result<()> {
    let Some(s) = seed().await? else {
        return Ok(());
    };
    seed_activity(&s).await?;
    let replay = load_replay_steerer(&s.handle).await?;
    let item = ReplayItem {
        turn_id: s.turn_id,
        trigger_request_id: s.request_id,
    };

    for budget in [VerifierInputBudget::Uncapped, VerifierInputBudget::V1] {
        let live = build_verifier_judge_context(
            &s.handle,
            s.session_uuid,
            Some(s.session_db_id),
            Some(s.turn_id),
            budget,
        )
        .await;
        let rebuilt = assemble(&s.handle, &item, &replay, budget).await?.context;

        assert_eq!(rebuilt.accumulated_ask, live.accumulated_ask, "{budget:?}");
        assert_eq!(
            rebuilt.trajectory_digest, live.trajectory_digest,
            "{budget:?}"
        );
        assert_eq!(
            rebuilt.session_action_list, live.session_action_list,
            "{budget:?}"
        );
        assert_eq!(rebuilt.intake_criteria, live.intake_criteria, "{budget:?}");
        assert!(!live.session_action_list.is_empty());
    }

    s.handle.pool().close().await;
    s.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn replay_on_unknown_request_is_err() -> anyhow::Result<()> {
    let Some(s) = seed().await? else {
        return Ok(());
    };
    let replay = load_replay_steerer(&s.handle).await?;
    let item = ReplayItem {
        turn_id: s.turn_id,
        trigger_request_id: -1,
    };

    let result = assemble(&s.handle, &item, &replay, VerifierInputBudget::V1).await;
    assert!(result.is_err());

    let reports = replay_items(
        &s.handle,
        &replay,
        &[item],
        VerifierInputBudget::V1,
        &ReplayMode::DryRun,
    )
    .await;
    assert!(matches!(reports[0].outcome, ItemOutcome::LoadFailed { .. }));

    s.handle.pool().close().await;
    s.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn empty_human_prompt_falls_back_to_the_open_turn_ask() -> anyhow::Result<()> {
    let Some(s) = seed().await? else {
        return Ok(());
    };
    seed_activity(&s).await?;
    sqlx::query!(
        "UPDATE requests.requests SET human_prompt = '' WHERE id = $1",
        s.request_id,
    )
    .execute(s.handle.pool())
    .await?;
    sqlx::query!(
        "UPDATE memory.operator_turns
            SET user_prompt = 'fix the parity harness', closed_at = NULL, created_at = $2
          WHERE id = $1",
        s.turn_id,
        s.base,
    )
    .execute(s.handle.pool())
    .await?;
    let replay = load_replay_steerer(&s.handle).await?;
    let item = ReplayItem {
        turn_id: s.turn_id,
        trigger_request_id: s.request_id,
    };

    let inputs = assemble(&s.handle, &item, &replay, VerifierInputBudget::Uncapped).await?;
    assert!(
        inputs
            .user_message
            .starts_with("User ask:\nfix the parity harness"),
        "{}",
        inputs.user_message
    );

    s.handle.pool().close().await;
    s.db.cleanup().await?;
    Ok(())
}

async fn note_sent_at(
    s: &Seeded,
    steerer_id: i64,
    text: &str,
    at: DateTime<Utc>,
) -> anyhow::Result<()> {
    sqlx::query!(
        "INSERT INTO steering.interventions
                (steerer_id, session_id, verdict_id, mode_id, authority_id, uuid, expires_at,
                 payload_text, delivered_request_id, created_at)
         SELECT $1, $2, v.id, m.id, a.id, gen_random_uuid(), $4::timestamptz + interval '10 minutes', $3, $5, $4
           FROM steering.verdicts v CROSS JOIN steering.modes m CROSS JOIN steering.authorities a
          WHERE v.code = 'fired' AND m.code = 'augment' AND a.code = 'redirect'",
        steerer_id,
        s.session_db_id,
        text,
        at,
        s.request_id,
    )
    .execute(s.handle.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn the_judge_is_shown_the_notes_already_sent_and_what_opened_the_turn() -> anyhow::Result<()>
{
    let Some(s) = seed().await? else {
        return Ok(());
    };
    seed_activity(&s).await?;
    let replay = load_replay_steerer(&s.handle).await?;
    sqlx::query!(
        "UPDATE requests.requests SET completed_at = $2 WHERE id = $1",
        s.request_id,
        s.base + Duration::seconds(200),
    )
    .execute(s.handle.pool())
    .await?;
    sqlx::query!(
        "UPDATE memory.operator_turns
            SET opened_by = 'machine', closed_at = NULL, created_at = $2,
                user_prompt = '<task-notification><status>completed</status></task-notification>'
          WHERE id = $1",
        s.turn_id,
        s.base,
    )
    .execute(s.handle.pool())
    .await?;
    let before_the_judged_moment = s.base + Duration::seconds(50);
    let after_the_judged_moment = s.base + Duration::seconds(300);
    note_sent_at(
        &s,
        replay.steerer.id,
        "check the export",
        before_the_judged_moment,
    )
    .await?;
    note_sent_at(
        &s,
        replay.steerer.id,
        "a later note",
        after_the_judged_moment,
    )
    .await?;
    let item = ReplayItem {
        turn_id: s.turn_id,
        trigger_request_id: s.request_id,
    };

    let inputs = assemble(&s.handle, &item, &replay, VerifierInputBudget::V1).await?;

    let message = &inputs.user_message;
    assert!(message.ends_with(&inputs.session_block), "{message}");
    assert!(message.contains(EARLIER_NOTES_HEADING), "{message}");
    assert!(
        message.contains("from verifier: check the export"),
        "{message}"
    );
    assert!(
        !message.contains("a later note"),
        "a note sent after the judged moment was not known then: {message}"
    );
    assert!(
        message.contains(&format!(
            "{TURN_OPENING_HEADING}\nthe result of a background task, not a message from the operator"
        )),
        "{message}"
    );
    assert!(message.contains(RECENT_REACTIONS_HEADING), "{message}");
    assert!(message.contains(EARLIER_TURNS_HEADING), "{message}");
    assert_eq!(
        note_policy::after_verdict(Some(&inputs.notes), Need::Medium),
        Some(NoteRefusal::NeedBelowLevel),
        "a session that had a note in the last hour only gets one that is needed more"
    );
    assert_eq!(
        note_policy::after_verdict(Some(&inputs.notes), Need::High),
        None
    );

    s.handle.pool().close().await;
    s.db.cleanup().await?;
    Ok(())
}
