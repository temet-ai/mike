use anyhow::Context;
use chrono::{DateTime, Duration, Utc};
use mike::dossier::genuine::{CLASSIFIER_VERSION, CaptureRule, PromptClass, PromptKind};
use mike::store::persistence::prompt_classification::{
    ClassificationWrite, ReclassifyRow, capture_rule_code,
};
use mike::store::persistence::{
    NeedsJudgmentQuery, PersistHandle, fetch_sessions_needing_judgment,
};
use mike::store::test_support::{self, TestDb, TestDbHandle};
use sqlx::PgPool;

#[path = "prompt_classification/migration.rs"]
mod migration;
#[path = "prompt_classification/streams.rs"]
mod streams;

struct Fixture {
    db: TestDbHandle,
    handle: PersistHandle,
    project_id: i64,
    session_id: i64,
    base: DateTime<Utc>,
}

async fn fixture() -> anyhow::Result<Option<Fixture>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping prompt_classification test");
        return Ok(None);
    };
    let db = test_db.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project_id = test_support::seed_project(handle.pool(), "prompt-classification").await?;
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

async fn insert_session(
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

struct NewRequest<'a> {
    id: i64,
    session_id: i64,
    project_id: i64,
    at: DateTime<Utc>,
    human_prompt: &'a str,
    author: &'a str,
    agent_id: Option<&'a str>,
}

async fn insert_request(pool: &PgPool, r: &NewRequest<'_>) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO requests.requests (
             id, session_id, project_id, method_id, path, is_streaming,
             message_count, is_tool_loop, status_id, params,
             last_user_message, human_prompt, turn_type_id, turn_origin_id,
             created_at, author_kind_id, claude_code_agent_id
         )
         OVERRIDING SYSTEM VALUE
         SELECT $1, $2, $3, rm.id, '/v1/messages', false,
                0, false, rs.id, '{}'::jsonb,
                '', $5, rtt.id, rto.id,
                $4, ak.id, $7
           FROM requests.methods rm, requests.statuses rs,
                requests.turn_types rtt, requests.turn_origins rto,
                requests.author_kinds ak
          WHERE rm.code = 'POST' AND rs.code = 'complete'
            AND rtt.code = 'user_prompt' AND rto.code = 'primary'
            AND ak.code = $6",
    )
    .bind(r.id)
    .bind(r.session_id)
    .bind(r.project_id)
    .bind(r.at)
    .bind(r.human_prompt)
    .bind(r.author)
    .bind(r.agent_id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn plain_request(f: &Fixture, id: i64, offset_secs: i64, text: &str) -> anyhow::Result<()> {
    insert_request(
        f.handle.pool(),
        &NewRequest {
            id,
            session_id: f.session_id,
            project_id: f.project_id,
            at: f.base + Duration::seconds(offset_secs),
            human_prompt: text,
            author: "operator",
            agent_id: None,
        },
    )
    .await
}

async fn insert_turn(
    f: &Fixture,
    session_id: i64,
    start_request_id: i64,
    opened_by: Option<&str>,
    at: DateTime<Utc>,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.operator_turns
             (session_id, project_id, start_request_id, end_request_id,
              created_at, closed_at, opened_by, user_prompt)
         VALUES ($1, $2, $3, $3, $4, $4, $5, 'prompt text')
         RETURNING id",
    )
    .bind(session_id)
    .bind(f.project_id)
    .bind(start_request_id)
    .bind(at)
    .bind(opened_by)
    .fetch_one(f.handle.pool())
    .await?;
    Ok(id)
}

type ClassificationRow = (String, Option<String>, Option<String>, Option<String>);

async fn stored_classification(
    pool: &PgPool,
    request_id: i64,
) -> anyhow::Result<Option<ClassificationRow>> {
    let row = sqlx::query_as(
        "SELECT k.code, c.rule, c.repeat_key, c.classifier_version
           FROM prompt_kinds.classifications c
           JOIN prompt_kinds.kinds k ON k.id = c.kind_id
          WHERE c.request_id = $1",
    )
    .bind(request_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

fn write_of<'a>(f: &Fixture, request_id: i64, kind_code: &'a str) -> ClassificationWrite<'a> {
    ClassificationWrite {
        request_id,
        session_db_id: f.session_id,
        kind_code,
        rule: None,
        repeat_key: None,
        classifier_version: "genuine-v1",
    }
}

#[tokio::test]
async fn write_prompt_classification_stores_kind_rule_repeat_key_and_version() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    plain_request(&f, 9001, 0, "Tool loaded.").await?;
    let class = PromptClass {
        kind: PromptKind::ToolLoaded,
        rule: Some(CaptureRule::A),
    };
    f.handle
        .write_prompt_classification(9001, f.session_id, &class, Some("tool loaded."))
        .await?;
    let stored = stored_classification(f.handle.pool(), 9001)
        .await?
        .context("classification row must exist")?;
    assert_eq!(stored.0, "tool-loaded");
    assert_eq!(stored.1.as_deref(), Some(capture_rule_code(CaptureRule::A)));
    assert_eq!(stored.2.as_deref(), Some("tool loaded."));
    assert_eq!(stored.3.as_deref(), Some(CLASSIFIER_VERSION));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn write_prompt_classification_unknown_kind_code_is_err() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    plain_request(&f, 9001, 0, "hello").await?;
    let outcome = f
        .handle
        .write_classification_row(&write_of(&f, 9001, "no-such-kind"))
        .await;
    assert!(outcome.is_err(), "an unknown kind code must be an error");
    assert!(
        stored_classification(f.handle.pool(), 9001)
            .await?
            .is_none()
    );
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn write_prompt_classification_second_write_for_same_request_is_noop() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    plain_request(&f, 9001, 0, "hello").await?;
    f.handle
        .write_classification_row(&write_of(&f, 9001, "operator"))
        .await?;
    f.handle
        .write_classification_row(&write_of(&f, 9001, "loop-wakeup"))
        .await?;
    let stored = stored_classification(f.handle.pool(), 9001)
        .await?
        .context("classification row must exist")?;
    assert_eq!(stored.0, "operator");
    f.db.cleanup().await?;
    Ok(())
}

fn reclassify(request_id: i64, session_id: i64, kind: &str, key: Option<&str>) -> ReclassifyRow {
    ReclassifyRow {
        request_id,
        session_id,
        kind: kind.to_string(),
        rule: None,
        repeat_key: key.map(str::to_string),
        version: "genuine-v1".to_string(),
    }
}

#[tokio::test]
async fn reclassify_batch_updates_only_changed_rows() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    plain_request(&f, 9001, 0, "one").await?;
    plain_request(&f, 9002, 10, "two").await?;
    for id in [9001, 9002] {
        f.handle
            .write_classification_row(&write_of(&f, id, "operator"))
            .await?;
    }
    let same = reclassify(9001, f.session_id, "operator", None);
    let changed = reclassify(9002, f.session_id, "loop-wakeup", Some("two"));
    let outcome = f.handle.reclassify_batch(&[same, changed]).await?;
    assert_eq!((outcome.updated, outcome.inserted), (1, 0));
    let first = stored_classification(f.handle.pool(), 9001).await?;
    let second = stored_classification(f.handle.pool(), 9002).await?;
    assert_eq!(first.map(|r| r.0).as_deref(), Some("operator"));
    assert_eq!(second.map(|r| r.0).as_deref(), Some("loop-wakeup"));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn reclassify_batch_inserts_missing_rows_in_one_statement() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    plain_request(&f, 9001, 0, "one").await?;
    plain_request(&f, 9002, 10, "two").await?;
    let rows = [
        reclassify(9001, f.session_id, "operator", None),
        reclassify(9002, f.session_id, "hook-feedback", None),
    ];
    let outcome = f.handle.reclassify_batch(&rows).await?;
    assert_eq!((outcome.updated, outcome.inserted), (0, 2));
    let again = f.handle.reclassify_batch(&rows).await?;
    assert_eq!((again.updated, again.inserted), (0, 0));
    f.db.cleanup().await?;
    Ok(())
}

async fn turn_fixture(f: &Fixture, opened_by: Option<&str>) -> anyhow::Result<i64> {
    plain_request(f, 9001, 0, "prompt").await?;
    insert_turn(f, f.session_id, 9001, opened_by, f.base).await
}

async fn claimed_ids(f: &Fixture) -> anyhow::Result<Vec<i64>> {
    let rows = f.handle.claim_unjudged_turns(50, "rubric-x").await?;
    Ok(rows.into_iter().map(|r| r.turn_id).collect())
}

#[tokio::test]
async fn claim_unjudged_turns_skips_machine_segments() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = turn_fixture(&f, Some("machine")).await?;
    assert!(!claimed_ids(&f).await?.contains(&turn));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn claim_unjudged_turns_skips_agent_opened_rows() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = turn_fixture(&f, Some("agent")).await?;
    assert!(!claimed_ids(&f).await?.contains(&turn));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn claim_unjudged_turns_keeps_rows_with_null_opened_by() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = turn_fixture(&f, None).await?;
    assert!(claimed_ids(&f).await?.contains(&turn));
    let operator = insert_turn(&f, f.session_id, 9001, Some("operator"), f.base).await?;
    assert!(claimed_ids(&f).await?.contains(&operator));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn sessions_needing_judgment_ignores_machine_segment_turns() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let quiet = insert_session(f.handle.pool(), f.project_id, f.base).await?;
    let busy = insert_session(f.handle.pool(), f.project_id, f.base).await?;
    plain_request(&f, 9001, 0, "machine").await?;
    plain_request(&f, 9002, 10, "operator").await?;
    insert_turn(&f, quiet, 9001, Some("machine"), f.base).await?;
    insert_turn(&f, busy, 9002, None, f.base).await?;
    let settled_before = f.base + Duration::hours(1);
    let query = NeedsJudgmentQuery {
        judge_model: "model-x",
        rubric_version: 1,
        settled_before,
        limit: 50,
        max_attempts_per_day: 3,
    };
    let rows = fetch_sessions_needing_judgment(f.handle.pool(), &query).await?;
    let selected: Vec<i64> = rows.iter().map(|r| r.session_db_id).collect();
    assert!(selected.contains(&busy));
    assert!(!selected.contains(&quiet));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn opened_by_check_rejects_unknown_value() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    plain_request(&f, 9001, 0, "prompt").await?;
    let outcome = insert_turn(&f, f.session_id, 9001, Some("robot"), f.base).await;
    assert!(
        outcome.is_err(),
        "opened_by outside the closed set must fail"
    );
    let bad_rule = sqlx::query(
        "INSERT INTO prompt_kinds.classifications (request_id, session_id, kind_id, rule)
         SELECT 9001, $1, k.id, 'zzz' FROM prompt_kinds.kinds k WHERE k.code = 'operator'",
    )
    .bind(f.session_id)
    .execute(f.handle.pool())
    .await;
    assert!(bad_rule.is_err(), "rule outside the closed set must fail");
    f.db.cleanup().await?;
    Ok(())
}
