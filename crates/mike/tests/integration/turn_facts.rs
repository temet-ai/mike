use chrono::{DateTime, Duration, Utc};
use mike::store::persistence::PersistHandle;
use mike::store::persistence::turn_facts::TurnFactsRow;
use mike::store::test_support::{self, TestDb, TestDbHandle};
use sqlx::PgPool;

#[path = "turn_facts/backfill.rs"]
mod backfill_cli;
#[path = "turn_facts/backstop.rs"]
mod backstop;
#[path = "turn_facts/checks.rs"]
mod checks;
#[path = "turn_facts/final_text_refresh.rs"]
mod final_text_refresh;
#[path = "turn_facts/lag.rs"]
mod lag;
#[path = "turn_facts/migration.rs"]
mod migration;
#[path = "turn_facts/rebuild.rs"]
mod rebuild;
#[path = "turn_facts/resolution.rs"]
pub(crate) mod resolution;
#[path = "turn_facts/settings.rs"]
mod settings;
#[path = "turn_facts/stop.rs"]
mod stop;
#[path = "turn_facts/text.rs"]
mod text;
#[path = "turn_facts/write.rs"]
mod write;

pub(crate) struct Fixture {
    pub(crate) db: TestDbHandle,
    pub(crate) handle: PersistHandle,
    pub(crate) project_id: i64,
    pub(crate) session_id: i64,
    pub(crate) base: DateTime<Utc>,
}

pub(crate) async fn fixture() -> anyhow::Result<Option<Fixture>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping turn_facts test");
        return Ok(None);
    };
    let db = test_db.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project_id = test_support::seed_project(handle.pool(), "turn-facts").await?;
    let session_id = insert_session(handle.pool(), project_id, Utc::now()).await?;
    let base =
        DateTime::from_timestamp_micros((Utc::now() - Duration::hours(2)).timestamp_micros())
            .unwrap_or_else(Utc::now);
    Ok(Some(Fixture {
        db,
        handle,
        project_id,
        session_id,
        base,
    }))
}

pub(crate) async fn enable_builder(f: &Fixture) -> anyhow::Result<()> {
    sqlx::query("UPDATE config.feature_toggles SET enabled = true WHERE key = 'dossier_builder'")
        .execute(f.handle.pool())
        .await?;
    Ok(())
}

pub(crate) async fn insert_session(
    pool: &PgPool,
    project_id: i64,
    last_seen: DateTime<Utc>,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES (gen_random_uuid(), $1, $2, $2) RETURNING id",
    )
    .bind(project_id)
    .bind(last_seen)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub(crate) struct Req<'a> {
    pub(crate) id: i64,
    pub(crate) origin: &'a str,
    pub(crate) turn_type: &'a str,
    pub(crate) prompt: &'a str,
    pub(crate) at_secs: i64,
    pub(crate) completed_secs: Option<i64>,
}

impl<'a> Req<'a> {
    pub(crate) fn primary(id: i64, at_secs: i64, prompt: &'a str) -> Self {
        Self {
            id,
            origin: "primary",
            turn_type: "user_prompt",
            prompt,
            at_secs,
            completed_secs: None,
        }
    }

    pub(crate) fn lane(id: i64, origin: &'a str, at_secs: i64) -> Self {
        Self {
            id,
            origin,
            turn_type: "tool_loop",
            prompt: "",
            at_secs,
            completed_secs: None,
        }
    }
}

pub(crate) async fn insert_request(f: &Fixture, r: &Req<'_>) -> anyhow::Result<()> {
    insert_request_in(f, f.session_id, r).await
}

pub(crate) async fn insert_request_in(
    f: &Fixture,
    session_id: i64,
    r: &Req<'_>,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO requests.requests (
             id, session_id, project_id, method_id, path, is_streaming,
             message_count, is_tool_loop, status_id, params,
             last_user_message, human_prompt, turn_type_id, turn_origin_id,
             created_at, completed_at
         )
         OVERRIDING SYSTEM VALUE
         SELECT $1, $2, $3, rm.id, '/v1/messages', false,
                0, false, rs.id, '{}'::jsonb,
                '', $6, rtt.id, rto.id,
                $7, $8
           FROM requests.methods rm, requests.statuses rs,
                requests.turn_types rtt, requests.turn_origins rto
          WHERE rm.code = 'POST' AND rs.code = 'complete'
            AND rtt.code = $4 AND rto.code = $5",
    )
    .bind(r.id)
    .bind(session_id)
    .bind(f.project_id)
    .bind(r.turn_type)
    .bind(r.origin)
    .bind(r.prompt)
    .bind(f.base + Duration::seconds(r.at_secs))
    .bind(r.completed_secs.map(|s| f.base + Duration::seconds(s)))
    .execute(f.handle.pool())
    .await?;
    Ok(())
}

pub(crate) async fn insert_text_block(
    f: &Fixture,
    request_id: i64,
    seq_ord: i32,
    content: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO responses.stream_blocks (
             request_id, session_id, seq_ord, content, byte_size, content_sha256, block_type_id)
         SELECT $1, $2, $3, $4, octet_length($4), repeat('a', 64), bt.id
           FROM responses.stream_block_types bt
          WHERE bt.code = 'text'",
    )
    .bind(request_id)
    .bind(f.session_id)
    .bind(seq_ord)
    .bind(content)
    .execute(f.handle.pool())
    .await?;
    Ok(())
}

pub(crate) async fn insert_response(
    f: &Fixture,
    request_id: i64,
    stop_reason: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO responses.responses (request_id, response_status, stop_reason_id, completed_at)
         SELECT $1, 200, sr.id, NOW()
           FROM responses.stop_reasons sr
          WHERE sr.code = $2",
    )
    .bind(request_id)
    .bind(stop_reason)
    .execute(f.handle.pool())
    .await?;
    Ok(())
}

pub(crate) struct NewTurn<'a> {
    pub(crate) start_request_id: i64,
    pub(crate) end_request_id: i64,
    pub(crate) at_secs: i64,
    pub(crate) closed_secs: Option<i64>,
    pub(crate) evidence_request_id: Option<i64>,
    pub(crate) final_text: Option<&'a str>,
}

impl NewTurn<'_> {
    pub(crate) fn closed(start: i64, end: i64, at_secs: i64, closed_secs: i64) -> Self {
        Self {
            start_request_id: start,
            end_request_id: end,
            at_secs,
            closed_secs: Some(closed_secs),
            evidence_request_id: None,
            final_text: None,
        }
    }
}

pub(crate) async fn insert_turn(f: &Fixture, t: &NewTurn<'_>) -> anyhow::Result<i64> {
    insert_turn_in(f, f.session_id, t).await
}

pub(crate) async fn insert_turn_in(
    f: &Fixture,
    session_id: i64,
    t: &NewTurn<'_>,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.operator_turns
             (session_id, project_id, start_request_id, end_request_id,
              created_at, closed_at, outcome_evidence_request_id,
              final_assistant_text, user_prompt)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'prompt')
         RETURNING id",
    )
    .bind(session_id)
    .bind(f.project_id)
    .bind(t.start_request_id)
    .bind(t.end_request_id)
    .bind(f.base + Duration::seconds(t.at_secs))
    .bind(t.closed_secs.map(|s| f.base + Duration::seconds(s)))
    .bind(t.evidence_request_id)
    .bind(t.final_text)
    .fetch_one(f.handle.pool())
    .await?;
    Ok(id)
}

pub(crate) async fn attach(f: &Fixture, turn_id: i64, request_ids: &[i64]) -> anyhow::Result<()> {
    sqlx::query("UPDATE requests.requests SET operator_turn_id = $1 WHERE id = ANY($2)")
        .bind(turn_id)
        .bind(request_ids)
        .execute(f.handle.pool())
        .await?;
    Ok(())
}

pub(crate) async fn dev_split(f: &Fixture, turn_id: i64) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO eval.turn_splits (operator_turn_id, split, split_reason)
         VALUES ($1, 'dev', 'hash') ON CONFLICT (operator_turn_id) DO NOTHING",
    )
    .bind(turn_id)
    .execute(f.handle.pool())
    .await?;
    Ok(())
}

pub(crate) async fn test_import_batch(f: &Fixture) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO eval.import_batches (source_key, manifest, counts, imported_by)
         VALUES ('test-seed', '{}', '{}', 'test')
         ON CONFLICT (source_key) DO UPDATE SET imported_by = EXCLUDED.imported_by
         RETURNING id",
    )
    .fetch_one(f.handle.pool())
    .await?)
}

pub(crate) fn resolve(row: &mut TurnFactsRow) {
    row.next_state = "resolved".to_owned();
    row.next_genuine_request_id = Some(row.max_request_id + 1);
}

pub(crate) fn sample_row(turn_id: i64, session_id: i64, built_at: DateTime<Utc>) -> TurnFactsRow {
    TurnFactsRow {
        operator_turn_id: turn_id,
        session_id,
        builder_version: "turn-facts-test".to_string(),
        built_at,
        max_request_id: 1,
        prompt_kind: "genuine".to_string(),
        prompt_rule: None,
        prompt_kind_at_close: "genuine".to_string(),
        prompt_rule_at_close: None,
        next_state: "pending".to_string(),
        next_genuine_request_id: None,
        next_genuine_turn_id: None,
        next_genuine_at: None,
        next_state_at_close: "pending".to_string(),
        next_genuine_request_id_at_close: None,
        skipped_before_next: serde_json::json!({}),
        reply_window_segment_ids: Vec::new(),
        reclassified_at: None,
        text_chars: 0,
        text_cut_at: 0,
        last_text_origin: None,
        text_spans: serde_json::json!([]),
        stop_request_id: None,
        stop_reason: None,
        aborted: false,
        interrupted: false,
        close_cause: "next_prompt".to_string(),
        checks_run: serde_json::json!([]),
        checks_count: 0,
        checks_failed: 0,
        ended_on: "other".to_string(),
        ended_on_version: "ended-on-v1".to_string(),
        session_class: "operator".to_string(),
        session_class_rule: "default".to_string(),
    }
}
