use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use super::delivery::attention_message_uuid;
use super::registry::Registry;
use crate::change_feed::Drained;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::request_write::IdMode;
use crate::store::persistence::test_support::acquire_ephemeral_db;

struct Item<'a> {
    kind: &'a str,
    task: &'a str,
    owner: Option<&'a str>,
    state: &'a str,
    launched_at: DateTime<Utc>,
}

async fn add_item(persist: &PersistHandle, session: i64, item: Item<'_>) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO agents.work_items
             (session_id, wire_tool_use_id, kind, task_id, owner_agent_id, state, launched_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(session)
    .bind(format!("toolu_{}", Uuid::new_v4().simple()))
    .bind(item.kind)
    .bind(item.task)
    .bind(item.owner)
    .bind(item.state)
    .bind(item.launched_at)
    .execute(persist.pool())
    .await?;
    Ok(())
}

async fn add_request(
    persist: &PersistHandle,
    session_uuid: &str,
    agent: Option<&str>,
    at: DateTime<Utc>,
) -> anyhow::Result<()> {
    let mut req = crate::store::tests::make_test_request_with_timestamp(None, session_uuid, at);
    req.claude_code_agent_id = agent.map(str::to_owned);
    persist
        .insert_request_initial(&req, true, IdMode::Ensure)
        .await?;
    Ok(())
}

async fn orphaned_job(persist: &PersistHandle) -> anyhow::Result<(String, i64)> {
    let uuid = Uuid::new_v4().to_string();
    let session = persist.ensure_session_exists(&uuid).await?;
    let launched_at = Utc::now() - Duration::minutes(30);
    let agent = Item {
        kind: "agent",
        task: "a1",
        owner: None,
        state: "completed",
        launched_at,
    };
    add_item(persist, session, agent).await?;
    let job = Item {
        kind: "bash",
        task: "j1",
        owner: Some("a1"),
        state: "running",
        launched_at,
    };
    add_item(persist, session, job).await?;
    Ok((uuid, session))
}

async fn attention_ids(persist: &PersistHandle, session: i64) -> anyhow::Result<Vec<i64>> {
    Ok(sqlx::query_scalar(
        "SELECT id FROM agents.work_item_attention WHERE session_id = $1 ORDER BY id",
    )
    .bind(session)
    .fetch_all(persist.pool())
    .await?)
}

fn everything() -> Drained {
    Drained {
        sessions: Vec::new(),
        everything: true,
    }
}

fn dirty(uuid: &str) -> Drained {
    Drained {
        sessions: vec![uuid.to_owned()],
        everything: false,
    }
}

#[tokio::test]
async fn step_known_finding_quiet_and_dirty_passes_write_nothing() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let (uuid, session) = orphaned_job(&persist).await?;
    let mut registry = Registry::default();

    registry.step(&persist, everything(), Utc::now()).await?;
    assert_eq!(attention_ids(&persist, session).await?.len(), 1);

    sqlx::query("DELETE FROM agents.work_item_attention WHERE session_id = $1")
        .bind(session)
        .execute(persist.pool())
        .await?;
    registry
        .step(&persist, Drained::default(), Utc::now())
        .await?;
    registry.step(&persist, dirty(&uuid), Utc::now()).await?;
    assert!(
        attention_ids(&persist, session).await?.is_empty(),
        "a finding already recorded is not upserted again"
    );

    registry.step(&persist, everything(), Utc::now()).await?;
    assert_eq!(
        attention_ids(&persist, session).await?.len(),
        1,
        "a resync forgets what it knew and records again"
    );
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn step_deliver_mode_mails_a_finding_once() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    sqlx::query(
        "INSERT INTO config.settings (key, value) VALUES ('work_registry.wake_mode', '\"deliver\"')
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
    )
    .execute(persist.pool())
    .await?;
    let (uuid, session) = orphaned_job(&persist).await?;
    let mut registry = Registry::default();

    registry.step(&persist, everything(), Utc::now()).await?;
    let ids = attention_ids(&persist, session).await?;
    let [id] = ids.as_slice() else {
        anyhow::bail!("expected one finding, got {ids:?}");
    };
    let mailed = || async {
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mailbox.messages WHERE message_uuid = $1",
        )
        .bind(attention_message_uuid(*id))
        .fetch_one(persist.pool())
        .await
    };
    assert_eq!(mailed().await?, 1);

    registry.step(&persist, dirty(&uuid), Utc::now()).await?;
    registry
        .step(&persist, Drained::default(), Utc::now())
        .await?;
    assert_eq!(
        mailed().await?,
        1,
        "a delivered finding is not mailed again"
    );
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn step_incremental_reads_match_the_full_ledger() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let uuid = Uuid::new_v4().to_string();
    let now = Utc::now();
    add_request(&persist, &uuid, None, now - Duration::minutes(20)).await?;
    add_request(&persist, &uuid, Some("a1"), now - Duration::minutes(30)).await?;
    let session = persist.ensure_session_exists(&uuid).await?;
    let agent = Item {
        kind: "agent",
        task: "a1",
        owner: None,
        state: "running",
        launched_at: now - Duration::minutes(40),
    };
    add_item(&persist, session, agent).await?;
    let mut registry = Registry::default();

    registry.step(&persist, everything(), now).await?;
    assert_same_ledger(&persist, &registry, session, &uuid).await?;
    let subjects = attention_subjects(&registry, session)?;
    assert!(subjects.iter().any(|s| s.starts_with("agent_silent")));

    add_request(&persist, &uuid, Some("a1"), Utc::now()).await?;
    registry.step(&persist, dirty(&uuid), Utc::now()).await?;
    assert_same_ledger(&persist, &registry, session, &uuid).await?;
    assert!(
        !attention_subjects(&registry, session)?
            .iter()
            .any(|s| s.starts_with("agent_silent")),
        "the new request is read incrementally and the agent is no longer silent"
    );
    db.cleanup().await?;
    Ok(())
}

fn attention_subjects(registry: &Registry, session: i64) -> anyhow::Result<Vec<String>> {
    let ledger = registry
        .cache
        .ledger(session, Utc::now())
        .ok_or_else(|| anyhow::anyhow!("session {session} is not cached"))?;
    Ok(ledger
        .attention
        .iter()
        .map(|a| format!("{}:{}", a.kind, a.subject))
        .collect())
}

async fn assert_same_ledger(
    persist: &PersistHandle,
    registry: &Registry,
    session: i64,
    uuid: &str,
) -> anyhow::Result<()> {
    let full = persist.work_item_ledger(session, uuid).await?;
    let cached = registry
        .cache
        .ledger(session, Utc::now())
        .ok_or_else(|| anyhow::anyhow!("session {session} is not cached"))?;
    let last = |l: &crate::work_items::ledger::Ledger| {
        l.agents
            .iter()
            .map(|a| (a.agent_id.clone(), a.state.clone(), a.last_request_at))
            .collect::<Vec<_>>()
    };
    assert_eq!(last(&cached), last(&full));
    assert_eq!(cached.attention, full.attention);
    Ok(())
}
