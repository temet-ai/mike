use uuid::Uuid;

use super::PersistHandle;
use super::test_support::{TestDb, seed_project, seed_session_turns, seed_two_sessions};

struct Seeded {
    handle: PersistHandle,
    busy_id: i64,
    single_id: i64,
    empty_id: i64,
}

async fn session_db_id(handle: &PersistHandle, uuid: &str) -> anyhow::Result<i64> {
    let id = sqlx::query_scalar!(
        "SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid",
        uuid,
    )
    .fetch_one(handle.pool())
    .await?;
    Ok(id)
}

async fn seeded() -> anyhow::Result<Option<Seeded>> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = factory.acquire().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = seed_project(handle.pool(), &format!("/latest/{}", Uuid::new_v4())).await?;
    let busy = Uuid::new_v4().to_string();
    let single = Uuid::new_v4().to_string();
    let empty = Uuid::new_v4().to_string();
    seed_two_sessions(handle.pool(), &busy, &single, project).await?;
    seed_two_sessions(handle.pool(), &empty, &Uuid::new_v4().to_string(), project).await?;
    seed_session_turns(handle.pool(), &busy, project, 3).await?;
    seed_session_turns(handle.pool(), &single, project, 1).await?;
    Ok(Some(Seeded {
        busy_id: session_db_id(&handle, &busy).await?,
        single_id: session_db_id(&handle, &single).await?,
        empty_id: session_db_id(&handle, &empty).await?,
        handle,
    }))
}

async fn true_max(
    handle: &PersistHandle,
    session_db_id: i64,
) -> anyhow::Result<Option<chrono::DateTime<chrono::Utc>>> {
    Ok(sqlx::query_scalar!(
        "SELECT max(created_at) FROM requests.requests WHERE session_id = $1",
        session_db_id,
    )
    .fetch_one(handle.pool())
    .await?)
}

#[tokio::test]
async fn latest_request_at_session_with_requests_returns_newest_created_at() -> anyhow::Result<()> {
    let Some(s) = seeded().await? else {
        return Ok(());
    };
    let latest = s.handle.latest_request_at(s.busy_id).await?;
    assert!(latest.is_some());
    assert_eq!(latest, true_max(&s.handle, s.busy_id).await?);
    Ok(())
}

#[tokio::test]
async fn latest_request_at_session_without_requests_returns_none() -> anyhow::Result<()> {
    let Some(s) = seeded().await? else {
        return Ok(());
    };
    assert_eq!(s.handle.latest_request_at(s.empty_id).await?, None);
    Ok(())
}

#[tokio::test]
async fn latest_request_at_many_maps_each_session_and_omits_those_without_requests()
-> anyhow::Result<()> {
    let Some(s) = seeded().await? else {
        return Ok(());
    };
    let found = s
        .handle
        .latest_request_at_many(&[s.busy_id, s.single_id, s.empty_id])
        .await?;
    assert_eq!(found.len(), 2);
    assert_eq!(
        found.get(&s.busy_id).copied(),
        true_max(&s.handle, s.busy_id).await?
    );
    assert_eq!(
        found.get(&s.single_id).copied(),
        true_max(&s.handle, s.single_id).await?
    );
    assert!(!found.contains_key(&s.empty_id));
    Ok(())
}

#[tokio::test]
async fn latest_request_at_many_empty_input_returns_empty_map() -> anyhow::Result<()> {
    let Some(s) = seeded().await? else {
        return Ok(());
    };
    assert!(s.handle.latest_request_at_many(&[]).await?.is_empty());
    Ok(())
}
