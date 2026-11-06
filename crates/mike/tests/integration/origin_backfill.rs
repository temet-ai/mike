use chrono::{DateTime, Duration, Utc};
use mike::store::persistence::PersistHandle;
use mike::store::persistence::origin_backfill::RelanePlanRow;
use mike::store::test_support::{self, TestDb, TestDbHandle};
use sqlx::PgPool;

const WORKFLOW_SYSTEM: &str =
    "You are Claude Code.\nYou are a subagent spawned by a workflow orchestration script.";
const PLAIN_SYSTEM: &str = "You are Claude Code.\nYou are a general-purpose agent.";
const HARNESS_FIRST_MESSAGE: &str = "  [Workflow harness — run 7] do the thing";

struct Fixture {
    _db: TestDbHandle,
    handle: PersistHandle,
    session_id: i64,
    project_id: i64,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
}

struct SeedRequest<'a> {
    origin: &'a str,
    turn_type: &'a str,
    agent_id: Option<&'a str>,
    system: &'a str,
    first_message: &'a str,
    author: &'a str,
    at: DateTime<Utc>,
}

async fn fixture() -> anyhow::Result<Option<Fixture>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping origin_backfill test");
        return Ok(None);
    };
    let db = test_db.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project_id = test_support::seed_project(handle.pool(), "origin-backfill").await?;
    let session_id: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES (gen_random_uuid(), $1, now(), now()) RETURNING id",
    )
    .bind(project_id)
    .fetch_one(handle.pool())
    .await?;
    let until = Utc::now() + Duration::hours(1);
    Ok(Some(Fixture {
        _db: db,
        handle,
        session_id,
        project_id,
        since: until - Duration::hours(5),
        until,
    }))
}

async fn seed(fx: &Fixture, req: SeedRequest<'_>) -> anyhow::Result<i64> {
    let body = serde_json::json!({
        "system": [{"type": "text", "text": req.system}],
        "messages": [{"role": "user", "content": [{"type": "text", "text": req.first_message}]}],
    })
    .to_string();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming, message_count,
            is_tool_loop, status_id, params, last_user_message, human_prompt,
            turn_type_id, turn_origin_id, author_kind_id, claude_code_agent_id,
            request_body, created_at)
         SELECT $1, $2, rm.id, '/v1/messages', false, 1, false, rs.id, '{}'::jsonb,
                'x', 'x', tt.id, ro.id, ak.id, $3, convert_to($4, 'UTF8'), $5
           FROM requests.methods rm, requests.statuses rs, requests.turn_types tt,
                requests.turn_origins ro, requests.author_kinds ak
          WHERE rm.code = 'POST' AND rs.code = 'complete' AND tt.code = $6
            AND ro.code = $7 AND ak.code = $8
         RETURNING id",
    )
    .bind(fx.session_id)
    .bind(fx.project_id)
    .bind(req.agent_id)
    .bind(body)
    .bind(req.at)
    .bind(req.turn_type)
    .bind(req.origin)
    .bind(req.author)
    .fetch_one(fx.handle.pool())
    .await?;
    Ok(id)
}

async fn seed_turn(fx: &Fixture, start: i64, closed_by: Option<i64>) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.operator_turns
            (session_id, project_id, start_request_id, end_request_id, user_prompt,
             closed_at, outcome_evidence_request_id)
         VALUES ($1, $2, $3, $3, 'x', CASE WHEN $4::bigint IS NULL THEN NULL ELSE now() END, $4)
         RETURNING id",
    )
    .bind(fx.session_id)
    .bind(fx.project_id)
    .bind(start)
    .bind(closed_by)
    .fetch_one(fx.handle.pool())
    .await?;
    Ok(id)
}

async fn origin_and_author(pool: &PgPool, id: i64) -> anyhow::Result<(String, String)> {
    let row = sqlx::query_as(
        "SELECT o.code, a.code FROM requests.requests r
           JOIN requests.turn_origins o ON o.id = r.turn_origin_id
           JOIN requests.author_kinds a ON a.id = r.author_kind_id
          WHERE r.id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

struct Seeded {
    workflow_opener: i64,
    harness_prefix: i64,
    plain_loop: i64,
    main_request: i64,
    agent_turn: i64,
    closed_turn: i64,
}

async fn seed_all(fx: &Fixture) -> anyhow::Result<Seeded> {
    let base = fx.since + Duration::hours(1);
    let main_request = seed(
        fx,
        SeedRequest {
            origin: "primary",
            turn_type: "user_prompt",
            agent_id: None,
            system: PLAIN_SYSTEM,
            first_message: "hello",
            author: "operator",
            at: base,
        },
    )
    .await?;
    let closed_turn = seed_turn(fx, main_request, None).await?;
    let plain_loop = seed(
        fx,
        SeedRequest {
            origin: "primary",
            turn_type: "tool_loop",
            agent_id: Some("agent-1"),
            system: PLAIN_SYSTEM,
            first_message: "hello",
            author: "harness",
            at: base + Duration::minutes(1),
        },
    )
    .await?;
    sqlx::query(
        "UPDATE memory.operator_turns SET closed_at = now(), outcome_evidence_request_id = $2 WHERE id = $1",
    )
    .bind(closed_turn)
    .bind(plain_loop)
    .execute(fx.handle.pool())
    .await?;
    let workflow_opener = seed(
        fx,
        SeedRequest {
            origin: "primary",
            turn_type: "user_prompt",
            agent_id: Some("agent-2"),
            system: WORKFLOW_SYSTEM,
            first_message: "go",
            author: "operator",
            at: base + Duration::minutes(2),
        },
    )
    .await?;
    let agent_turn = seed_turn(fx, workflow_opener, None).await?;
    let harness_prefix = seed(
        fx,
        SeedRequest {
            origin: "primary_first",
            turn_type: "user_prompt",
            agent_id: Some("agent-3"),
            system: PLAIN_SYSTEM,
            first_message: HARNESS_FIRST_MESSAGE,
            author: "operator",
            at: base + Duration::minutes(3),
        },
    )
    .await?;
    Ok(Seeded {
        workflow_opener,
        harness_prefix,
        plain_loop,
        main_request,
        agent_turn,
        closed_turn,
    })
}

async fn run_backfill(fx: &Fixture, execute: bool) -> anyhow::Result<(Vec<RelanePlanRow>, u64)> {
    let window = (fx.since, fx.until);
    let candidates = fx
        .handle
        .origin_backfill_candidates(window, (fx.since, 0), 100)
        .await?;
    let ids: Vec<i64> = candidates.iter().map(|(id, _)| *id).collect();
    let plan = fx.handle.origin_backfill_plan(&ids).await?;
    let mut written = 0;
    if execute {
        written = fx.handle.origin_backfill_relane(&plan, window).await?;
        let turns = fx.handle.origin_backfill_unmarked_turns(&ids).await?;
        let turn_ids: Vec<i64> = turns.iter().map(|t| t.turn_id).collect();
        written += fx
            .handle
            .origin_backfill_mark_agent_opened(&turn_ids)
            .await?;
    }
    Ok((plan, written))
}

#[tokio::test]
async fn origin_backfill_dry_run_plans_lanes_and_writes_nothing() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let seeded = seed_all(&fx).await?;
    let (plan, written) = run_backfill(&fx, false).await?;
    assert_eq!(written, 0);
    let origin_of = |id: i64| {
        plan.iter()
            .find(|row| row.id == id)
            .map(|row| row.new_origin.clone())
    };
    assert_eq!(
        origin_of(seeded.workflow_opener).as_deref(),
        Some("workflow_subagent")
    );
    assert_eq!(
        origin_of(seeded.harness_prefix).as_deref(),
        Some("workflow_subagent")
    );
    assert_eq!(origin_of(seeded.plain_loop).as_deref(), Some("subagent"));
    assert_eq!(
        origin_of(seeded.main_request),
        None,
        "main-lane request has no agent id"
    );
    let (origin, _) = origin_and_author(fx.handle.pool(), seeded.plain_loop).await?;
    assert_eq!(origin, "primary");
    Ok(())
}

#[tokio::test]
async fn origin_backfill_execute_relanes_marks_agent_turns_and_is_idempotent() -> anyhow::Result<()>
{
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let seeded = seed_all(&fx).await?;
    let (_, written) = run_backfill(&fx, true).await?;
    assert_eq!(written, 4, "3 requests re-laned plus 1 agent-opened turn");
    let pool = fx.handle.pool();
    assert_eq!(
        origin_and_author(pool, seeded.workflow_opener).await?,
        ("workflow_subagent".into(), "agent".into())
    );
    assert_eq!(
        origin_and_author(pool, seeded.harness_prefix).await?.0,
        "workflow_subagent"
    );
    assert_eq!(
        origin_and_author(pool, seeded.plain_loop).await?,
        ("subagent".into(), "agent".into())
    );
    assert_eq!(
        origin_and_author(pool, seeded.main_request).await?.0,
        "primary"
    );
    let opened_by: Option<String> =
        sqlx::query_scalar("SELECT opened_by FROM memory.operator_turns WHERE id = $1")
            .bind(seeded.agent_turn)
            .fetch_one(pool)
            .await?;
    assert_eq!(opened_by.as_deref(), Some("agent"));
    let untouched: Option<String> =
        sqlx::query_scalar("SELECT opened_by FROM memory.operator_turns WHERE id = $1")
            .bind(seeded.closed_turn)
            .fetch_one(pool)
            .await?;
    assert_eq!(untouched, None);
    let (plan_again, written_again) = run_backfill(&fx, true).await?;
    assert!(plan_again.is_empty());
    assert_eq!(written_again, 0);
    Ok(())
}

#[tokio::test]
async fn origin_backfill_lists_turns_closed_by_an_agent_request() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let seeded = seed_all(&fx).await?;
    let closed = fx
        .handle
        .origin_backfill_closed_by_agent((fx.since, fx.until))
        .await?;
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0].turn_id, seeded.closed_turn);
    assert_eq!(closed[0].closing_request_id, seeded.plain_loop);
    assert_eq!(closed[0].start_request_id, seeded.main_request);
    Ok(())
}
