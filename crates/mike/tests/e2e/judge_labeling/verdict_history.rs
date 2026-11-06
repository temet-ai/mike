use crate::db_helpers;

use mike::analysis::JUDGE_RUBRIC_VERSION;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use wiremock::MockServer;

use super::support::{mount_judge_stub, seed_unjudged_turns, start_proxy_with_judge};

const OVERRIDE_PROMPT: &str = "Judge the turn strictly. Return the label schema.";

async fn live_verdicts(
    pool: &PgPool,
    session_id: &str,
) -> anyhow::Result<Vec<(String, String, bool)>> {
    Ok(sqlx::query_as(
        "SELECT v.rubric_version, v.input_text, v.projected
           FROM memory.turn_verdicts v
           JOIN sessions.sessions s ON s.id = v.session_id
          WHERE s.session_uuid = $1::text::uuid AND v.mode = 'live'
          ORDER BY v.id",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?)
}

async fn stamped_versions(pool: &PgPool, session_id: &str) -> anyhow::Result<Vec<Option<String>>> {
    Ok(sqlx::query_scalar(
        "SELECT t.judge_rubric_version
           FROM memory.operator_turns t
           JOIN sessions.sessions s ON s.id = t.session_id
          WHERE s.session_uuid = $1::text::uuid
          ORDER BY t.id",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?)
}

async fn sent_user_messages(or_mock: &MockServer) -> anyhow::Result<Vec<String>> {
    let requests = or_mock.received_requests().await.unwrap_or_default();
    let mut messages = Vec::new();
    for request in requests {
        let body: serde_json::Value = serde_json::from_slice(&request.body)?;
        if let Some(text) = body.pointer("/messages/1/content").and_then(|v| v.as_str()) {
            messages.push(text.to_owned());
        }
    }
    Ok(messages)
}

async fn store_prompt_override(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO config.settings (key, value) VALUES ('judge_worker.system_prompt', $1)
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
    )
    .bind(serde_json::Value::String(OVERRIDE_PROMPT.to_owned()))
    .execute(pool)
    .await?;
    Ok(())
}

fn sha12(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .take(6)
        .map(|b| format!("{b:02x}"))
        .collect()
}

struct Swept {
    _db: mike::store::test_support::TestDbHandle,
    pool: PgPool,
    or_mock: MockServer,
}

async fn run_sweep(session_id: &str, with_override: bool) -> anyhow::Result<Option<Swept>> {
    let upstream_mock = MockServer::start().await;
    let or_mock = MockServer::start().await;
    let Some(factory) = db_helpers::TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let test_db = factory.acquire_isolated().await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;
    mike::llm::reset_global_for_tests();
    seed_unjudged_turns(&pool, "test-verdict-history", session_id, 5).await?;
    if with_override {
        store_prompt_override(&pool).await?;
    }
    mount_judge_stub(&or_mock).await;
    let (_proxy, shutdown, state) =
        start_proxy_with_judge(&upstream_mock.uri(), &test_db.database_url, &or_mock.uri()).await?;
    mike::judge_worker::run_judge_once(&state).await?;
    shutdown.cancel();
    Ok(Some(Swept {
        _db: test_db,
        pool,
        or_mock,
    }))
}

#[tokio::test]
async fn live_sweep_appends_one_projected_live_verdict_per_judged_turn_with_the_plain_r3_id()
-> anyhow::Result<()> {
    let session_id = "12345678-f009-4f09-8f09-f09f09f09f01";
    let Some(Swept { _db, pool, or_mock }) = run_sweep(session_id, false).await? else {
        return Ok(());
    };
    let verdicts = live_verdicts(&pool, session_id).await?;
    assert_eq!(verdicts.len(), 5);
    assert!(
        verdicts
            .iter()
            .all(|(rubric, _, projected)| rubric == JUDGE_RUBRIC_VERSION && *projected)
    );
    let stamped = stamped_versions(&pool, session_id).await?;
    assert!(
        stamped
            .iter()
            .all(|v| v.as_deref() == Some(JUDGE_RUBRIC_VERSION))
    );
    let mut sent = sent_user_messages(&or_mock).await?;
    let mut stored: Vec<String> = verdicts.into_iter().map(|(_, input, _)| input).collect();
    sent.sort();
    stored.sort();
    assert_eq!(
        stored, sent,
        "each input_text equals the user message sent to OpenRouter"
    );
    Ok(())
}

#[tokio::test]
async fn live_sweep_with_prompt_override_stamps_r3_plus_sha12_on_both_records() -> anyhow::Result<()>
{
    let session_id = "12345678-f009-4f09-8f09-f09f09f09f02";
    let Some(Swept { _db, pool, .. }) = run_sweep(session_id, true).await? else {
        return Ok(());
    };
    let expected = format!("{JUDGE_RUBRIC_VERSION}+{}", sha12(OVERRIDE_PROMPT));
    let verdicts = live_verdicts(&pool, session_id).await?;
    assert_eq!(verdicts.len(), 5);
    assert!(verdicts.iter().all(|(rubric, _, _)| *rubric == expected));
    let stamped = stamped_versions(&pool, session_id).await?;
    assert!(
        stamped
            .iter()
            .all(|v| v.as_deref() == Some(expected.as_str()))
    );
    Ok(())
}
