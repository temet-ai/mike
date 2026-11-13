use anyhow::Context;
use mike::store::persistence::PersistHandle;
use mike::store::persistence::acceptance_criteria::{AskSeen, NewCriterion};
use mike::store::persistence::dossier_repo::DossierPersist as _;
use mike::store::persistence::memory_ask_ledger::NewAskLedgerEntry;
use mike::store::persistence::steerer_repo::SteererPersist as _;
use sqlx::PgPool;

use crate::test_db::TestDb;

async fn seed_session(pool: &PgPool) -> anyhow::Result<i64> {
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, NOW(), NOW()) RETURNING id",
    )
    .bind(format!("issue152-criteria-{}", uuid::Uuid::new_v4()))
    .fetch_one(pool)
    .await?;
    let session_fk: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, NOW(), NOW()) RETURNING id",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    Ok(session_fk)
}

fn criterion(text: &str, provenance: &str, span: Option<&str>) -> NewCriterion {
    NewCriterion {
        criterion: text.to_owned(),
        provenance: provenance.to_owned(),
        operator_span: span.map(str::to_owned),
    }
}

#[tokio::test]
async fn acceptance_criteria_versioning_and_provenance_lifecycle() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;
    let session = seed_session(&pool).await?;

    handle
        .steerer()
        .insert_acceptance_criteria_map(
            session,
            AskSeen::count_only(1),
            Some("model-a"),
            None,
            &[criterion(
                "The comms panel is reachable from the session page.",
                "operator",
                Some("add a comms panel"),
            )],
        )
        .await?;
    handle
        .steerer()
        .insert_acceptance_criteria_map(
            session,
            AskSeen::count_only(2),
            Some("model-a"),
            None,
            &[
                criterion(
                    "The comms panel is reachable from the session page.",
                    "operator",
                    Some("add a comms panel"),
                ),
                criterion(
                    "It is built on the current shared primitives.",
                    "inferred",
                    None,
                ),
            ],
        )
        .await?;

    let current = handle
        .steerer()
        .fetch_current_acceptance_criteria(session)
        .await?
        .context("a map was stored")?;
    assert_eq!(
        current.version, 2,
        "the newest version is what grades a stop"
    );
    assert_eq!(current.criteria.len(), 2);
    assert_eq!(current.ask_entry_count, 2);

    let history = handle
        .steerer()
        .fetch_acceptance_criteria_history(session)
        .await?;
    assert_eq!(history.len(), 2, "the earlier version must still exist");
    assert_eq!(history[0].version, 2, "newest first");
    assert_eq!(history[1].version, 1);
    assert_eq!(
        history[1].criteria.len(),
        1,
        "the old version keeps its own criteria, so the diff is real"
    );

    let map2 = handle
        .steerer()
        .fetch_current_acceptance_criteria(session)
        .await?
        .context("stored")?;
    assert_eq!(map2.criteria[0].provenance, "operator");
    assert_eq!(
        map2.criteria[0].operator_span.as_deref(),
        Some("add a comms panel")
    );
    assert_eq!(map2.criteria[1].provenance, "inferred");
    assert_eq!(map2.criteria[1].operator_span, None, "nothing to quote");
    assert_eq!(map2.criteria[0].ord, 0, "order is stable");
    assert_eq!(map2.criteria[1].ord, 1);

    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn acceptance_criteria_refuses_unbacked_provenance_and_stores_empty_authoring()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;
    let session = seed_session(&pool).await?;

    let unbacked = handle
        .steerer()
        .insert_acceptance_criteria_map(
            session,
            AskSeen::count_only(1),
            None,
            None,
            &[criterion("It is reachable.", "operator", None)],
        )
        .await;
    assert!(
        unbacked.is_err(),
        "an `operator` criterion with no quoted words must be refused"
    );

    let inflated = handle
        .steerer()
        .insert_acceptance_criteria_map(
            session,
            AskSeen::count_only(1),
            None,
            None,
            &[criterion("It is reachable.", "inferred", Some("words"))],
        )
        .await;
    assert!(
        inflated.is_err(),
        "an `inferred` criterion cannot carry operator words — that pairing is a lie either way"
    );

    let empty_session = seed_session(&pool).await?;
    handle
        .steerer()
        .insert_acceptance_criteria_map(empty_session, AskSeen::count_only(1), None, None, &[])
        .await?;
    let map = handle
        .steerer()
        .fetch_current_acceptance_criteria(empty_session)
        .await?
        .context("an empty map is still a map")?;
    assert!(map.criteria.is_empty());
    assert_eq!(map.ask_entry_count, 1);

    pool.close().await;
    Ok(())
}

async fn seed_session_with_uuid(pool: &PgPool) -> anyhow::Result<(i64, uuid::Uuid)> {
    let session = seed_session(pool).await?;
    let session_uuid: uuid::Uuid =
        sqlx::query_scalar("SELECT session_uuid FROM sessions.sessions WHERE id = $1")
            .bind(session)
            .fetch_one(pool)
            .await?;
    Ok((session, session_uuid))
}

async fn append_ask(handle: &PersistHandle, session: i64, content: &str) -> anyhow::Result<i64> {
    handle
        .dossier()
        .append_ask_ledger_entry(&NewAskLedgerEntry {
            session_id: session,
            request_id: None,
            operator_turn_id: None,
            kind: "clarification",
            kind_source: "keyword",
            content,
        })
        .await?
        .context("the ask was appended")
}

#[tokio::test]
async fn ask_seen_tracks_the_newest_ask_past_the_fetch_cap() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;
    let (session, session_uuid) = seed_session_with_uuid(&pool).await?;

    assert_eq!(
        handle
            .steerer()
            .fetch_acceptance_criteria_ask_seen(session)
            .await?,
        None,
        "a session with no map has seen nothing"
    );

    let mut newest = 0;
    for n in 0..45 {
        newest = append_ask(&handle, session, &format!("ask {n}")).await?;
    }
    let fetched = handle
        .dossier()
        .fetch_ask_ledger_for_model(session_uuid, 40)
        .await?;
    assert_eq!(fetched.len(), 40, "the fetch is capped");
    assert_eq!(
        fetched.iter().map(|e| e.id).max(),
        Some(newest),
        "the cap drops the oldest asks, never the newest"
    );
    assert_eq!(
        handle
            .dossier()
            .count_ask_ledger_for_model(session_uuid)
            .await?,
        45,
        "the count is not capped by the fetch"
    );

    let stored = AskSeen {
        ask_entry_count: 40,
        newest_ask_entry_id: Some(newest),
    };
    handle
        .steerer()
        .insert_acceptance_criteria_map(session, stored, Some("model-a"), None, &[])
        .await?;
    assert_eq!(
        handle
            .steerer()
            .fetch_acceptance_criteria_ask_seen(session)
            .await?,
        Some(stored)
    );

    let next = append_ask(&handle, session, "one more").await?;
    let refetched = handle
        .dossier()
        .fetch_ask_ledger_for_model(session_uuid, 40)
        .await?;
    assert_eq!(refetched.len(), 40, "the stored count did not move");
    assert!(
        refetched.iter().map(|e| e.id).max() > stored.newest_ask_entry_id,
        "the newest id moved past the one the map saw ({next})"
    );

    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn ask_seen_reports_the_newest_map_and_an_unrecorded_newest_ask_as_none() -> anyhow::Result<()>
{
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;
    let session = seed_session(&pool).await?;

    handle
        .steerer()
        .insert_acceptance_criteria_map(session, AskSeen::count_only(3), None, None, &[])
        .await?;
    let first = handle
        .steerer()
        .fetch_acceptance_criteria_ask_seen(session)
        .await?;
    assert_eq!(first.map(|s| s.ask_entry_count), Some(3));
    assert_eq!(first.and_then(|s| s.newest_ask_entry_id), None);

    let newer = AskSeen {
        ask_entry_count: 4,
        newest_ask_entry_id: Some(77),
    };
    handle
        .steerer()
        .insert_acceptance_criteria_map(session, newer, None, None, &[])
        .await?;
    assert_eq!(
        handle
            .steerer()
            .fetch_acceptance_criteria_ask_seen(session)
            .await?,
        Some(newer),
        "the newest version answers"
    );

    pool.close().await;
    Ok(())
}
