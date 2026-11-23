use std::time::Duration;

use sqlx::postgres::PgListener;
use uuid::Uuid;

use crate::change_feed::ChangeFeed;
use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::steerer_repo::SteererPersist as _;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db, seed_project};

struct Machines {
    db: TestDbHandle,
    here: PersistHandle,
    other: PersistHandle,
    feed: ChangeFeed,
    steerer_id: i64,
}

async fn machines() -> anyhow::Result<Option<Machines>> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(None);
    };
    let here = PersistHandle::connect(&db.database_url).await?;
    let other = PersistHandle::connect(&db.database_url).await?;
    let feed = ChangeFeed::new();
    here.attach_change_feed(feed.clone());
    let steerer_id: i64 = sqlx::query_scalar(
        "SELECT id FROM steering.steerers WHERE deleted_at IS NULL ORDER BY id LIMIT 1",
    )
    .fetch_one(here.pool())
    .await?;
    Ok(Some(Machines {
        db,
        here,
        other,
        feed,
        steerer_id,
    }))
}

async fn seed_session(pool: &sqlx::PgPool) -> anyhow::Result<Uuid> {
    let session = Uuid::new_v4();
    let project = seed_project(pool, &format!("/enrolment-memo/{session}")).await?;
    sqlx::query(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, NOW(), NOW())",
    )
    .bind(session)
    .bind(project)
    .execute(pool)
    .await?;
    Ok(session)
}

async fn enrol(handle: &PersistHandle, session: Uuid, steerer_id: i64) -> anyhow::Result<i64> {
    let mode: String = sqlx::query_scalar("SELECT code FROM steering.modes ORDER BY id LIMIT 1")
        .fetch_one(handle.pool())
        .await?;
    let authority: String =
        sqlx::query_scalar("SELECT code FROM steering.authorities ORDER BY id LIMIT 1")
            .fetch_one(handle.pool())
            .await?;
    Ok(handle
        .steerer()
        .insert_enrolment(session, steerer_id, &mode, &authority, "test")
        .await?)
}

async fn steerers_of(handle: &PersistHandle, session: Uuid) -> anyhow::Result<Vec<i64>> {
    Ok(handle
        .steerer()
        .list_active_enrolments_for_session(session)
        .await?
        .into_iter()
        .map(|e| e.steerer_id)
        .collect())
}

#[tokio::test]
async fn active_enrolments_answer_from_memory_until_the_sessions_enrolments_move()
-> anyhow::Result<()> {
    let Some(m) = machines().await? else {
        return Ok(());
    };
    let session = seed_session(m.here.pool()).await?;
    let neighbour = seed_session(m.here.pool()).await?;
    assert!(steerers_of(&m.here, session).await?.is_empty());
    enrol(&m.other, session, m.steerer_id).await?;
    assert!(
        steerers_of(&m.here, session).await?.is_empty(),
        "the other machine's enrolment has not reached the feed"
    );
    assert_eq!(
        steerers_of(&m.other, session).await?,
        [m.steerer_id],
        "a handle with no feed reads the table"
    );
    m.feed.record(
        RowChangedTable::SteeringEnrolments,
        Some(&neighbour.to_string()),
    );
    assert!(
        steerers_of(&m.here, session).await?.is_empty(),
        "another session's enrolments moving keeps this session's answer"
    );
    m.feed.record(
        RowChangedTable::SteeringEnrolments,
        Some(&session.to_string()),
    );
    assert_eq!(steerers_of(&m.here, session).await?, [m.steerer_id]);
    m.db.cleanup().await
}

#[tokio::test]
async fn own_enrolment_writes_drop_the_held_answer_at_once() -> anyhow::Result<()> {
    let Some(m) = machines().await? else {
        return Ok(());
    };
    let session = seed_session(m.here.pool()).await?;
    assert!(steerers_of(&m.here, session).await?.is_empty());
    let enrolment = enrol(&m.here, session, m.steerer_id).await?;
    assert_eq!(steerers_of(&m.here, session).await?, [m.steerer_id]);
    assert!(m.here.steerer().delete_enrolment(enrolment).await?);
    assert!(steerers_of(&m.here, session).await?.is_empty());
    m.db.cleanup().await
}

#[tokio::test]
async fn own_auto_enrolment_drops_the_held_answer_at_once() -> anyhow::Result<()> {
    let Some(m) = machines().await? else {
        return Ok(());
    };
    sqlx::query(
        "UPDATE steering.steerers SET auto_enrol = true, enabled = true, project_id = NULL
          WHERE id = $1",
    )
    .bind(m.steerer_id)
    .execute(m.here.pool())
    .await?;
    let session = seed_session(m.here.pool()).await?;
    assert!(steerers_of(&m.here, session).await?.is_empty());
    assert!(m.here.steerer().enrol_auto_steerers(session).await? > 0);
    assert!(steerers_of(&m.here, session).await?.contains(&m.steerer_id));
    m.db.cleanup().await
}

#[tokio::test]
async fn active_enrolments_memory_is_dropped_by_a_resync() -> anyhow::Result<()> {
    let Some(m) = machines().await? else {
        return Ok(());
    };
    let session = seed_session(m.here.pool()).await?;
    assert!(steerers_of(&m.here, session).await?.is_empty());
    enrol(&m.other, session, m.steerer_id).await?;
    m.feed.resync();
    assert_eq!(steerers_of(&m.here, session).await?, [m.steerer_id]);
    m.db.cleanup().await
}

async fn next_enrolment_notice(
    listener: &mut PgListener,
    session: Uuid,
) -> anyhow::Result<Option<String>> {
    let wanted = session.to_string();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while let Ok(Ok(note)) = tokio::time::timeout_at(deadline, listener.recv()).await {
        let payload: serde_json::Value = serde_json::from_str(note.payload())?;
        if payload["session_id"] == wanted.as_str() && payload["table"] == "steering_enrolments" {
            let table = serde_json::from_value::<RowChangedTable>(payload["table"].clone())?;
            assert_eq!(table, RowChangedTable::SteeringEnrolments);
            return Ok(payload["op"].as_str().map(str::to_owned));
        }
    }
    Ok(None)
}

#[tokio::test]
async fn enrolment_trigger_names_the_session_on_enrol_disenrol_and_session_delete()
-> anyhow::Result<()> {
    let Some(m) = machines().await? else {
        return Ok(());
    };
    let session = seed_session(m.here.pool()).await?;
    let mut listener = PgListener::connect(&m.db.database_url).await?;
    listener.listen("proxy.row_changed").await?;
    let enrolment = enrol(&m.other, session, m.steerer_id).await?;
    assert_eq!(
        next_enrolment_notice(&mut listener, session)
            .await?
            .as_deref(),
        Some("INSERT")
    );
    assert!(m.other.steerer().delete_enrolment(enrolment).await?);
    assert_eq!(
        next_enrolment_notice(&mut listener, session)
            .await?
            .as_deref(),
        Some("UPDATE")
    );
    sqlx::query("DELETE FROM sessions.sessions WHERE session_uuid = $1")
        .bind(session)
        .execute(m.other.pool())
        .await?;
    assert_eq!(
        next_enrolment_notice(&mut listener, session)
            .await?
            .as_deref(),
        Some("DELETE")
    );
    m.db.cleanup().await
}
