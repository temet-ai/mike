use chrono::{DateTime, Duration, Utc};
use mike::store::persistence::steerer_repo::SteererPersist as _;
use mike::store::persistence::{PersistError, PersistHandle};
use mike::store::test_support::{self, TestDb, TestDbHandle};

struct Seeded {
    db: TestDbHandle,
    handle: PersistHandle,
    session_db_id: i64,
    request_id: i64,
    turn_id: i64,
    base: DateTime<Utc>,
}

async fn seed() -> anyhow::Result<Option<Seeded>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = test_db.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project_id = test_support::seed_project(handle.pool(), "verifier-input-budget").await?;
    let session_uuid = uuid::Uuid::new_v4().to_string();
    let other_uuid = uuid::Uuid::new_v4().to_string();
    test_support::seed_two_sessions(handle.pool(), &session_uuid, &other_uuid, project_id).await?;
    test_support::seed_session_turns(handle.pool(), &session_uuid, project_id, 1).await?;
    let (session_db_id, request_id, turn_id): (i64, i64, i64) = sqlx::query_as(
        "SELECT s.id, r.id, t.id
           FROM sessions.sessions s
           JOIN requests.requests r ON r.session_id = s.id
           JOIN memory.operator_turns t ON t.start_request_id = r.id
          WHERE s.session_uuid = $1::text::uuid",
    )
    .bind(&session_uuid)
    .fetch_one(handle.pool())
    .await?;
    sqlx::query("UPDATE requests.requests SET operator_turn_id = $2 WHERE id = $1")
        .bind(request_id)
        .bind(turn_id)
        .execute(handle.pool())
        .await?;
    Ok(Some(Seeded {
        db,
        handle,
        session_db_id,
        request_id,
        turn_id,
        base: DateTime::from_timestamp_micros((Utc::now() - Duration::hours(1)).timestamp_micros())
            .unwrap_or_else(Utc::now),
    }))
}

async fn insert_use(
    s: &Seeded,
    i: i32,
    tool_name: &str,
    operation: &str,
    command: Option<&str>,
    at: DateTime<Utc>,
) -> anyhow::Result<i64> {
    let input = command.map(|c| serde_json::json!({ "command": c }));
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO tools.uses
            (wire_tool_use_id, request_id, session_id, ord, tool_name, input, operation, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING id",
    )
    .bind(format!("toolu_vib_{}_{i}", s.session_db_id))
    .bind(s.request_id)
    .bind(s.session_db_id)
    .bind(i)
    .bind(tool_name)
    .bind(input)
    .bind(operation)
    .bind(at)
    .fetch_one(s.handle.pool())
    .await?;
    Ok(id)
}

async fn insert_block(s: &Seeded, i: i32, arrived_at: DateTime<Utc>) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO responses.stream_blocks
            (request_id, session_id, block_type_id, seq_ord, content, content_sha256,
             wire_tool_use_id, arrived_at, byte_size)
         VALUES ($1, $2, 3, $3, '', $4, $5, $6, 0)",
    )
    .bind(s.request_id)
    .bind(s.session_db_id)
    .bind(i)
    .bind("0".repeat(64))
    .bind(format!("toolu_vib_{}_{i}", s.session_db_id))
    .bind(arrived_at)
    .execute(s.handle.pool())
    .await?;
    Ok(())
}

async fn insert_result(
    s: &Seeded,
    tool_use_id: i64,
    is_error: bool,
    at: DateTime<Utc>,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO tools.results
            (request_id, session_id, tool_use_id, ord, is_error, content, content_sha256, content_bytes, created_at)
         VALUES ($1, $6, $2, 0, $3, '\"ok\"'::jsonb, $4, 4, $5)",
    )
    .bind(s.request_id)
    .bind(tool_use_id)
    .bind(is_error)
    .bind("0".repeat(64))
    .bind(at)
    .bind(s.session_db_id)
    .execute(s.handle.pool())
    .await?;
    Ok(())
}

async fn seed_mixed_actions(s: &Seeded, n: i32) -> anyhow::Result<()> {
    for i in 0..n {
        let at = s.base + Duration::seconds(i64::from(i));
        let (name, op, cmd) = match i % 4 {
            0 => ("Read", "read", None),
            1 => ("Edit", "write", None),
            2 => ("Bash", "exec", Some("make test")),
            _ => ("Grep", "search", None),
        };
        let id = insert_use(s, i, name, op, cmd, at).await?;
        if name == "Bash" {
            insert_result(s, id, i % 8 == 2, at + Duration::milliseconds(500)).await?;
        }
    }
    Ok(())
}

#[tokio::test]
async fn recent_actions_return_newest_rows_in_session_order() -> anyhow::Result<()> {
    let Some(s) = seed().await? else {
        return Ok(());
    };
    seed_mixed_actions(&s, 40).await?;

    let rows = s
        .handle
        .steerer()
        .fetch_session_action_list_recent(s.session_db_id, 10)
        .await?;
    let full = s
        .handle
        .steerer()
        .fetch_session_action_list(s.session_db_id)
        .await?;

    assert_eq!(rows.len(), 10);
    let tail: Vec<(&str, Option<&str>)> = full[30..]
        .iter()
        .map(|r| (r.tool_name.as_str(), r.operation.as_deref()))
        .collect();
    let got: Vec<(&str, Option<&str>)> = rows
        .iter()
        .map(|r| (r.tool_name.as_str(), r.operation.as_deref()))
        .collect();
    assert_eq!(got, tail, "the newest 10 rows, oldest first");
    assert_eq!(rows[0].tool_name, "Bash");
    assert_eq!(rows[0].command.as_deref(), Some("make test"));

    let all = s
        .handle
        .steerer()
        .fetch_session_action_list_recent(s.session_db_id, 500)
        .await?;
    assert_eq!(
        all.len(),
        40,
        "a limit above the row count returns every row"
    );

    s.handle.pool().close().await;
    s.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn action_counts_match_full_list() -> anyhow::Result<()> {
    let Some(s) = seed().await? else {
        return Ok(());
    };
    seed_mixed_actions(&s, 40).await?;

    let counts = s
        .handle
        .steerer()
        .fetch_session_action_counts(s.session_db_id)
        .await?;
    let full = s
        .handle
        .steerer()
        .fetch_session_action_list(s.session_db_id)
        .await?;

    assert_eq!(counts.total, full.len() as i64);
    let reads = full
        .iter()
        .filter(|r| matches!(r.operation.as_deref(), Some("read" | "search" | "fetch")))
        .count() as i64;
    let writes = full
        .iter()
        .filter(|r| r.operation.as_deref() == Some("write"))
        .count() as i64;
    let commands = full.iter().filter(|r| r.tool_name == "Bash").count() as i64;
    let failed = full
        .iter()
        .filter(|r| r.tool_name == "Bash" && r.result_is_error == Some(true))
        .count() as i64;
    assert_eq!(counts.reads, reads);
    assert_eq!(counts.writes, writes);
    assert_eq!(counts.commands, commands);
    assert_eq!(counts.failed_commands, failed);
    assert_eq!(
        (counts.total, counts.commands, counts.failed_commands),
        (40, 10, 5)
    );

    let empty = s.handle.steerer().fetch_session_action_counts(-1).await?;
    assert_eq!(empty.total, 0);

    s.handle.pool().close().await;
    s.db.cleanup().await?;
    Ok(())
}

async fn seed_context_rows(s: &Seeded, trigger_at: DateTime<Utc>) -> anyhow::Result<()> {
    for (label, at) in [("before", -10i64), ("after", 10i64)] {
        sqlx::query(
            "INSERT INTO memory.ask_ledger_entries (session_id, operator_turn_id, kind, content, created_at)
             VALUES ($1, NULL, 'initial', $2, $3)",
        )
        .bind(s.session_db_id)
        .bind(format!("ask {label}"))
        .bind(trigger_at + Duration::seconds(at))
        .execute(s.handle.pool())
        .await?;
    }

    let verifier_id: i64 = sqlx::query_scalar(
        "SELECT id FROM steering.steerers WHERE kind = 'verifier' ORDER BY id LIMIT 1",
    )
    .fetch_one(s.handle.pool())
    .await?;
    for (label, at) in [("before", -20i64), ("after", 20i64)] {
        sqlx::query(
            "INSERT INTO steering.interventions
                (steerer_id, session_id, verdict_id, mode_id, authority_id, payload_text, off_thread_topic, created_at)
             SELECT $1, $2, v.id, st.default_mode_id, st.authority_id, $3, $3, $4
               FROM steering.verdicts v, steering.steerers st
              WHERE v.code = 'fired' AND st.id = $1",
        )
        .bind(verifier_id)
        .bind(s.session_db_id)
        .bind(format!("fire {label}"))
        .bind(trigger_at + Duration::seconds(at))
        .execute(s.handle.pool())
        .await?;
    }

    for (version, at) in [(1i32, -30i64), (2i32, 30i64)] {
        let map_id: i64 = sqlx::query_scalar(
            "INSERT INTO memory.acceptance_criteria_maps (session_id, version, ask_entry_count, created_at)
             VALUES ($1, $2, 1, $3) RETURNING id",
        )
        .bind(s.session_db_id)
        .bind(version)
        .bind(trigger_at + Duration::seconds(at))
        .fetch_one(s.handle.pool())
        .await?;
        sqlx::query(
            "INSERT INTO memory.acceptance_criteria (map_id, ord, criterion, provenance)
             VALUES ($1, 0, $2, 'inferred')",
        )
        .bind(map_id)
        .bind(format!("criterion v{version}"))
        .execute(s.handle.pool())
        .await?;
    }
    Ok(())
}

async fn assert_own_fire_excluded(
    s: &Seeded,
    verifier_id: i64,
    trigger_at: DateTime<Utc>,
    intervention_id: i64,
) -> anyhow::Result<()> {
    let own_trigger: Option<i64> =
        sqlx::query_scalar("SELECT trigger_request_id FROM steering.interventions WHERE id = $1")
            .bind(intervention_id)
            .fetch_one(s.handle.pool())
            .await?;
    if let Some(own_trigger) = own_trigger {
        let without_own = s
            .handle
            .steerer()
            .fetch_prior_verifier_fires_as_of(
                s.session_db_id,
                verifier_id,
                trigger_at,
                10,
                own_trigger,
            )
            .await?;
        assert!(
            without_own.is_empty(),
            "the evaluated call's own fire is excluded"
        );
    }
    Ok(())
}

#[tokio::test]
async fn replay_as_of_excludes_rows_after_trigger() -> anyhow::Result<()> {
    let Some(s) = seed().await? else {
        return Ok(());
    };
    let trigger_at = s.base + Duration::seconds(100);
    sqlx::query("UPDATE requests.requests SET created_at = $2, completed_at = $2 WHERE id = $1")
        .bind(s.request_id)
        .bind(trigger_at)
        .execute(s.handle.pool())
        .await?;

    let early = insert_use(&s, 0, "Bash", "exec", Some("cargo test"), s.base).await?;
    insert_result(&s, early, true, trigger_at + Duration::seconds(5)).await?;
    let settled = insert_use(
        &s,
        1,
        "Bash",
        "exec",
        Some("ls"),
        s.base + Duration::seconds(1),
    )
    .await?;
    insert_result(&s, settled, false, s.base + Duration::seconds(2)).await?;
    insert_use(&s, 2, "Edit", "write", None, s.base + Duration::seconds(2)).await?;
    insert_block(&s, 0, s.base).await?;
    insert_block(&s, 1, s.base + Duration::seconds(1)).await?;
    insert_block(&s, 2, trigger_at + Duration::seconds(10)).await?;

    seed_context_rows(&s, trigger_at).await?;

    let trigger = s
        .handle
        .steerer()
        .fetch_replay_trigger(s.request_id)
        .await?;
    assert_eq!(trigger.session_db_id, s.session_db_id);
    assert_eq!(trigger.trigger_at, trigger_at);

    let actions = s
        .handle
        .steerer()
        .fetch_session_action_list_as_of(s.session_db_id, trigger_at)
        .await?;
    assert_eq!(actions.len(), 2, "the Edit written after T is excluded");
    assert!(
        !actions[0].has_result,
        "a result written after T is not seen"
    );
    assert!(actions[1].has_result);

    let recent = s
        .handle
        .steerer()
        .fetch_session_action_list_recent_as_of(s.session_db_id, trigger_at, 5)
        .await?;
    assert_eq!(recent.len(), 2);
    let counts = s
        .handle
        .steerer()
        .fetch_session_action_counts_as_of(s.session_db_id, trigger_at)
        .await?;
    assert_eq!(
        (counts.total, counts.commands, counts.failed_commands),
        (2, 2, 0)
    );

    let calls = s
        .handle
        .steerer()
        .fetch_turn_tool_calls_as_of(s.turn_id, s.session_db_id, trigger_at)
        .await?;
    assert_eq!(calls.len(), 2, "the turn's uses written by T");
    assert_eq!(calls.iter().filter(|c| c.has_result).count(), 1);

    let ledger = s
        .handle
        .steerer()
        .fetch_ask_ledger_as_of(s.session_db_id, trigger_at, 30)
        .await?;
    assert_eq!(ledger.len(), 1);
    assert_eq!(ledger[0].content, "ask before");

    let verifier_id: i64 = sqlx::query_scalar(
        "SELECT id FROM steering.steerers WHERE kind = 'verifier' ORDER BY id LIMIT 1",
    )
    .fetch_one(s.handle.pool())
    .await?;
    let fires = s
        .handle
        .steerer()
        .fetch_prior_verifier_fires_as_of(s.session_db_id, verifier_id, trigger_at, 10, 0)
        .await?;
    assert_eq!(fires.len(), 1);
    assert_eq!(fires[0].payload_text.as_deref(), Some("fire before"));
    assert_eq!(fires[0].off_thread_topic.as_deref(), Some("fire before"));

    assert_own_fire_excluded(&s, verifier_id, trigger_at, fires[0].intervention_id).await?;

    let map = s
        .handle
        .steerer()
        .fetch_acceptance_criteria_as_of(s.session_db_id, trigger_at)
        .await?
        .ok_or_else(|| anyhow::anyhow!("expected the version 1 map"))?;
    assert_eq!(map.version, 1);
    assert_eq!(map.criteria[0].criterion, "criterion v1");

    s.handle.pool().close().await;
    s.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn replay_on_unknown_request_is_err() -> anyhow::Result<()> {
    let Some(s) = seed().await? else {
        return Ok(());
    };
    let result = s.handle.steerer().fetch_replay_trigger(-1).await;
    assert!(
        matches!(result, Err(PersistError::DataInvariant(_))),
        "an unknown trigger request is a DataInvariant error, got {result:?}"
    );
    s.handle.pool().close().await;
    s.db.cleanup().await?;
    Ok(())
}
