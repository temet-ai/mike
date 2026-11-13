use mike::store::persistence::PersistHandle;
use mike::store::persistence::dossier_repo::DossierPersist as _;
use mike::store::persistence::memory_ask_ledger::NewAskLedgerEntry;
use mike::store::persistence::test_support;
use uuid::Uuid;

use crate::test_db::TestDb;

fn entry<'a>(
    session_id: i64,
    operator_turn_id: Option<i64>,
    kind: &'a str,
    content: &'a str,
) -> NewAskLedgerEntry<'a> {
    NewAskLedgerEntry {
        session_id,
        request_id: None,
        operator_turn_id,
        kind,
        kind_source: "keyword",
        content,
    }
}

async fn seed_project_and_session(pool: &sqlx::PgPool) -> anyhow::Result<(i64, i64, Uuid)> {
    let project_id =
        test_support::seed_project(pool, &format!("proj-ask-ledger-{}", Uuid::new_v4())).await?;
    let session_uuid = Uuid::new_v4();
    let session_id: i64 = sqlx::query_scalar!(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, NOW(), NOW()) RETURNING id",
        session_uuid,
        project_id,
    )
    .fetch_one(pool)
    .await?;
    Ok((project_id, session_id, session_uuid))
}

#[tokio::test]
async fn append_then_fetch_orders_oldest_first_and_caps_to_the_most_recent_within_limit()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;

    let (_project_id, session_id, session_uuid) = seed_project_and_session(&pool).await?;

    handle
        .dossier()
        .append_ask_ledger_entry(&entry(session_id, None, "initial", "build the ask ledger"))
        .await?;
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    handle
        .dossier()
        .append_ask_ledger_entry(&entry(
            session_id,
            None,
            "clarification",
            "just the schema half",
        ))
        .await?;
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    handle
        .dossier()
        .append_ask_ledger_entry(&entry(
            session_id,
            None,
            "correction",
            "not the judge wiring",
        ))
        .await?;

    let entries = handle.dossier().fetch_ask_ledger(session_uuid, 10).await?;
    assert_eq!(entries.len(), 3, "all three entries must round-trip");
    assert_eq!(
        entries.iter().map(|e| e.kind.as_str()).collect::<Vec<_>>(),
        vec!["initial", "clarification", "correction"],
        "fetch must return oldest-first"
    );
    assert_eq!(entries[0].content, "build the ask ledger");
    assert_eq!(entries[0].session_id, session_id);
    assert!(entries[0].operator_turn_id.is_none());

    for i in 0..5 {
        handle
            .dossier()
            .append_ask_ledger_entry(&entry(
                session_id,
                None,
                "clarification",
                &format!("ask #{i}"),
            ))
            .await?;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    let capped = handle.dossier().fetch_ask_ledger(session_uuid, 3).await?;
    assert_eq!(
        capped
            .iter()
            .map(|e| e.content.as_str())
            .collect::<Vec<_>>(),
        vec!["ask #2", "ask #3", "ask #4"],
        "limit must keep the MOST RECENT rows, returned oldest-first"
    );

    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn kind_check_violation_rejects_invalid_kind() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;

    let (_project_id, session_id, _session_uuid) = seed_project_and_session(&pool).await?;

    let result = handle
        .dossier()
        .append_ask_ledger_entry(&entry(session_id, None, "bogus_kind", "should not insert"))
        .await;
    assert!(
        result.is_err(),
        "an out-of-vocabulary kind must be rejected by ask_ledger_entries_kind_check"
    );

    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn fk_violations_reject_unknown_session_and_unknown_operator_turn() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;

    let unknown_session = handle
        .dossier()
        .append_ask_ledger_entry(&entry(i64::MAX, None, "initial", "orphan write"))
        .await;
    assert!(
        unknown_session.is_err(),
        "a session_id with no sessions.sessions row must fail the FK constraint"
    );

    let (_project_id, session_id, _session_uuid) = seed_project_and_session(&pool).await?;
    let unknown_turn = handle
        .dossier()
        .append_ask_ledger_entry(&entry(
            session_id,
            Some(i64::MAX),
            "initial",
            "orphan turn ref",
        ))
        .await;
    assert!(
        unknown_turn.is_err(),
        "an operator_turn_id with no memory.operator_turns row must fail the FK constraint"
    );

    pool.close().await;
    Ok(())
}
