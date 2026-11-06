use chrono::Utc;
use mike::app::inspect_router_on;
use mike::config::{AppState, ProxyConfig};
use mike::store::persistence::PersistHandle;
use mike::store::persistence::turn_facts::TurnFactsRow;
use sqlx::PgPool;
use tokio::net::TcpListener;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::db_helpers;

pub(super) struct Promoted {
    pub(super) _db: mike::store::test_support::TestDbHandle,
    pub(super) pool: PgPool,
    pub(super) state: AppState,
    pub(super) or_mock: MockServer,
    pub(super) bundle_id: i64,
    pub(super) session_uuid: String,
    pub(super) turns: Vec<i64>,
}

fn v2_response() -> serde_json::Value {
    let content = serde_json::json!({
        "reaction": "corrected",
        "delivery": "partial",
        "confidence": 0.9,
        "rationale": "the operator says the retry was never tested",
        "unmet_ask": "run the tests",
        "evidence_quality": "asserted"
    })
    .to_string();
    serde_json::json!({
        "id": "chatcmpl-test",
        "object": "chat.completion",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": content}, "finish_reason": "stop"}]
    })
}

async fn setting(pool: &PgPool, key: &str, value: serde_json::Value) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO config.settings (key, value) VALUES ($1, $2)
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

async fn seed_turns(
    pool: &PgPool,
    session_uuid: &str,
    count: i64,
) -> anyhow::Result<(i64, Vec<i64>)> {
    let session_id: i64 = sqlx::query_scalar(
        "WITH p AS (
    SELECT id FROM projects.projects WHERE project_path = 'judge-promotion'
),
ins AS (
    INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
    SELECT 'judge-promotion', 'judge-promotion', now(), now()
    WHERE NOT EXISTS (SELECT 1 FROM p)
    RETURNING id
),
proj AS (
    SELECT id FROM p UNION ALL SELECT id FROM ins
)
INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
SELECT $1::text::uuid, proj.id, now(), now() FROM proj
RETURNING id",
    )
    .bind(session_uuid)
    .fetch_one(pool)
    .await?;
    let turns: Vec<i64> = sqlx::query_scalar(
        "WITH reqs AS (
    INSERT INTO requests.requests (
        session_id, project_id, method_id, path, is_streaming, message_count, is_tool_loop,
        status_id, params, last_user_message, human_prompt, turn_type_id, turn_origin_id,
        created_at, completed_at)
    SELECT s.id, s.project_id, rm.id, '/v1/messages', false, 0, false, rs.id, '{}'::jsonb,
           'add the retry', 'add the retry', rtt.id, rto.id,
           now() - interval '1 hour' + (g.i || ' seconds')::interval,
           now() - interval '1 hour' + (g.i || ' seconds')::interval
      FROM generate_series(1, $2::int) g(i), sessions.sessions s, requests.methods rm,
           requests.statuses rs, requests.turn_types rtt, requests.turn_origins rto
     WHERE s.id = $1 AND rm.code = 'POST' AND rs.code = 'complete'
       AND rtt.code = 'user_prompt' AND rto.code = 'primary'
    RETURNING id, created_at, session_id, project_id
)
INSERT INTO memory.operator_turns (session_id, project_id, start_request_id, end_request_id,
                                   created_at, closed_at, user_prompt, final_assistant_text)
SELECT r.session_id, r.project_id, r.id, r.id, r.created_at, r.created_at + interval '5 seconds',
       'add the retry', 'Retry added.'
  FROM reqs r
RETURNING id",
    )
    .bind(session_id)
    .bind(count)
    .fetch_all(pool)
    .await?;
    Ok((session_id, turns))
}

fn facts_row(turn: i64, session_id: i64, prompt_kind: &str) -> TurnFactsRow {
    TurnFactsRow {
        operator_turn_id: turn,
        session_id,
        builder_version: "turn-facts-test".to_string(),
        built_at: Utc::now(),
        max_request_id: 1,
        prompt_kind: prompt_kind.to_string(),
        prompt_rule: None,
        prompt_kind_at_close: prompt_kind.to_string(),
        prompt_rule_at_close: None,
        next_state: "resolved".to_string(),
        next_genuine_request_id: Some(2),
        next_genuine_turn_id: None,
        next_genuine_at: None,
        next_state_at_close: "resolved".to_string(),
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

async fn insert_bundle(pool: &PgPool) -> anyhow::Result<i64> {
    let components = mike::judge_worker::default_v2_bundle_components("google/gemini-3.5-flash");
    Ok(sqlx::query_scalar(
        "INSERT INTO eval.bundles (role, subject_key, origin, status, components, content_sha256, build_sha, promoted_at)
         VALUES ('turn_judge', '', 'candidate', 'candidate', $1, 'sha-promotion', 'build', now() - interval '2 hours')
         RETURNING id",
    )
    .bind(components)
    .fetch_one(pool)
    .await?)
}

async fn next_genuine_message(pool: &PgPool, session_id: i64) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming, message_count, is_tool_loop,
            status_id, params, last_user_message, human_prompt, turn_type_id, turn_origin_id,
            created_at, completed_at)
         SELECT s.id, s.project_id, rm.id, '/v1/messages', false, 0, false, rs.id, '{}'::jsonb,
                'that retry is still untested', 'that retry is still untested', rtt.id, rto.id,
                now() - interval '30 minutes', now() - interval '30 minutes'
           FROM sessions.sessions s, requests.methods rm, requests.statuses rs,
                requests.turn_types rtt, requests.turn_origins rto
          WHERE s.id = $1 AND rm.code = 'POST' AND rs.code = 'complete'
            AND rtt.code = 'user_prompt' AND rto.code = 'primary'",
    )
    .bind(session_id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn configure_live_judge(
    pool: &sqlx::PgPool,
    bundle_id: i64,
    live: bool,
) -> anyhow::Result<()> {
    setting(pool, "llm.rate_limit_per_minute", serde_json::json!(600)).await?;
    setting(pool, "llm.ceiling_max_calls", serde_json::json!(100_000)).await?;
    setting(pool, "turn_judge.immediate", serde_json::json!(true)).await?;
    setting(pool, "turn_judge.immediate_delay_ms", serde_json::json!(0)).await?;
    if live {
        setting(
            pool,
            "turn_judge.live_bundle_id",
            serde_json::json!(bundle_id),
        )
        .await?;
    }
    sqlx::query("UPDATE config.feature_toggles SET enabled = true, operator_set = true WHERE key = 'judge_worker'")
        .execute(pool)
        .await?;
    Ok(())
}

async fn judge_state(database_url: &str, openrouter_uri: String) -> anyhow::Result<AppState> {
    let mut config = ProxyConfig::try_parse_from([
        "mike",
        "--inspect-listen-addr",
        "127.0.0.1:0",
        "--persist-enabled",
        "1",
        "--database-url",
        database_url,
    ])?;
    config.set_openrouter_base_url(openrouter_uri);
    config.set_openrouter_api_key("test-api-key".to_owned());
    config.set_judge_enabled(true);
    AppState::new(config).await
}

pub(super) async fn promoted(
    session_uuid: &str,
    count: i64,
    live: bool,
) -> anyhow::Result<Option<Promoted>> {
    let or_mock = MockServer::start().await;
    let Some(factory) = db_helpers::TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = factory.acquire_isolated().await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    mike::llm::reset_global_for_tests();
    let (session_id, turns) = seed_turns(&pool, session_uuid, count).await?;
    next_genuine_message(&pool, session_id).await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let rows: Vec<TurnFactsRow> = turns
        .iter()
        .map(|t| facts_row(*t, session_id, "genuine"))
        .collect();
    handle.upsert_turn_facts(&rows).await?;
    let bundle_id = insert_bundle(&pool).await?;
    configure_live_judge(&pool, bundle_id, live).await?;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(v2_response()))
        .mount(&or_mock)
        .await;
    let state = judge_state(&db.database_url, or_mock.uri()).await?;
    Ok(Some(Promoted {
        _db: db,
        pool,
        state,
        or_mock,
        bundle_id,
        session_uuid: session_uuid.to_owned(),
        turns,
    }))
}

pub(super) async fn serve(state: &AppState) -> anyhow::Result<String> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let base = format!("http://{}", listener.local_addr()?);
    let router = inspect_router_on(state.clone(), listener.local_addr()?);
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    Ok(base)
}

pub(super) async fn judge_calls(or_mock: &MockServer) -> usize {
    or_mock.received_requests().await.map_or(0, |r| r.len())
}
