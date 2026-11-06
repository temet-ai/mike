use mike::store::persistence::PersistHandle;
use mike::store::persistence::id_resolvers::{
    resolve_agent_id_by_uuid, resolve_lesson_id_by_uuid, resolve_or_create_agent_id,
    resolve_session_id,
};

use crate::test_db::TestDb;

#[tokio::test]
async fn concurrent_session_resolution_returns_the_committed_winner() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let session = uuid::Uuid::new_v4().to_string();
    let mut first = handle.pool().begin().await?;
    let expected = resolve_session_id(&mut *first, &session, None, Some(1)).await?;
    let mut second = handle.pool().acquire().await?;
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *second)
        .await?;
    let competing =
        tokio::spawn(
            async move { resolve_session_id(&mut *second, &session, None, Some(2)).await },
        );
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let blocked: bool = sqlx::query_scalar("SELECT cardinality(pg_blocking_pids($1)) > 0")
                .bind(pid)
                .fetch_one(handle.pool())
                .await?;
            if blocked {
                return Ok::<_, sqlx::Error>(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await??;
    first.commit().await?;
    assert_eq!(competing.await??, expected);
    let admitted: Option<i32> =
        sqlx::query_scalar("SELECT flow_capture_version FROM sessions.sessions WHERE id = $1")
            .bind(expected)
            .fetch_one(handle.pool())
            .await?;
    assert_eq!(admitted, Some(1));
    Ok(())
}

#[tokio::test]
async fn resolving_existing_agent_is_case_insensitive_and_does_not_rewrite_it() -> anyhow::Result<()>
{
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let name = format!("pg19-agent-{}", uuid::Uuid::new_v4());
    let id = resolve_or_create_agent_id(handle.pool(), &name).await?;
    let before: String = sqlx::query_scalar("SELECT xmin::text FROM agents.agents WHERE id = $1")
        .bind(id)
        .fetch_one(handle.pool())
        .await?;
    assert_eq!(
        resolve_or_create_agent_id(handle.pool(), &name.to_uppercase()).await?,
        id
    );
    let after: String = sqlx::query_scalar("SELECT xmin::text FROM agents.agents WHERE id = $1")
        .bind(id)
        .fetch_one(handle.pool())
        .await?;
    assert_eq!(before, after);
    Ok(())
}

#[derive(sqlx::FromRow)]
struct FlowAdmissionColumns {
    flow_capture_version: Option<i32>,
    flow_admitted_at: Option<chrono::DateTime<chrono::Utc>>,
    flow_producer: Option<String>,
    flow_hook_event_name: Option<String>,
    flow_start_source: Option<String>,
}

async fn fetch_flow_admission_columns(
    pool: &sqlx::PgPool,
    session_id: &str,
) -> anyhow::Result<FlowAdmissionColumns> {
    Ok(sqlx::query_as::<_, FlowAdmissionColumns>(
        "SELECT flow_capture_version, flow_admitted_at, flow_producer,
                flow_hook_event_name, flow_start_source
         FROM sessions.sessions WHERE session_uuid = $1::text::uuid",
    )
    .bind(session_id)
    .fetch_one(pool)
    .await?)
}

#[tokio::test]
async fn resolve_session_id_finds_or_creates_and_binds_to_a_real_project() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let session_id = uuid::Uuid::new_v4().to_string();

    let id1 = resolve_session_id(handle.pool(), &session_id, None, None).await?;
    assert!(id1 > 0, "resolved session id must be a positive bigint");

    let id2 = resolve_session_id(handle.pool(), &session_id, None, None).await?;
    assert_eq!(id1, id2, "resolve_session_id must be idempotent");

    let (db_id,): (i64,) =
        sqlx::query_as("SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid")
            .bind(&session_id)
            .fetch_one(handle.pool())
            .await?;
    assert_eq!(db_id, id1, "resolved id must match the sessions row PK");

    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*)::bigint FROM sessions.sessions WHERE session_uuid = $1::text::uuid",
    )
    .bind(&session_id)
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(count, 1, "resolve_session_id must not insert duplicates");

    let session_id_b = uuid::Uuid::new_v4().to_string();
    let project_path_a = format!("/tmp/resolve-some-a-{}", uuid::Uuid::new_v4());
    let project_path_b = format!("/tmp/resolve-some-b-{}", uuid::Uuid::new_v4());
    let (pid_a,): (i64,) = sqlx::query_as(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, NOW(), NOW())
         RETURNING id",
    )
    .bind(&project_path_a)
    .fetch_one(handle.pool())
    .await?;
    let (pid_b,): (i64,) = sqlx::query_as(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, NOW(), NOW())
         RETURNING id",
    )
    .bind(&project_path_b)
    .fetch_one(handle.pool())
    .await?;

    let idb1 = resolve_session_id(handle.pool(), &session_id_b, Some(pid_a), None).await?;
    let (bound,): (i64,) = sqlx::query_as("SELECT project_id FROM sessions.sessions WHERE id = $1")
        .bind(idb1)
        .fetch_one(handle.pool())
        .await?;
    assert_eq!(
        bound, pid_a,
        "new session must be bootstrapped under the supplied real project"
    );

    let idb2 = resolve_session_id(handle.pool(), &session_id_b, Some(pid_b), None).await?;
    assert_eq!(
        idb1, idb2,
        "resolve_session_id must be idempotent per session"
    );
    let (still_bound,): (i64,) =
        sqlx::query_as("SELECT project_id FROM sessions.sessions WHERE id = $1")
            .bind(idb1)
            .fetch_one(handle.pool())
            .await?;
    assert_eq!(
        still_bound, pid_a,
        "find path must leave project_id unchanged (no real→real repoint)"
    );

    Ok(())
}

#[tokio::test]
async fn resolve_session_id_stamps_flow_admission_evidence_only_for_single_message_first_sighting()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;

    let single = uuid::Uuid::new_v4().to_string();
    resolve_session_id(handle.pool(), &single, None, Some(1)).await?;
    let evidence = fetch_flow_admission_columns(handle.pool(), &single).await?;
    assert_eq!(evidence.flow_capture_version, Some(1));
    assert!(evidence.flow_admitted_at.is_some());
    assert_eq!(evidence.flow_producer.as_deref(), Some("mike"));
    assert_eq!(evidence.flow_hook_event_name.as_deref(), Some("capture"));
    assert_eq!(evidence.flow_start_source.as_deref(), Some("capture"));

    let multi = uuid::Uuid::new_v4().to_string();
    resolve_session_id(handle.pool(), &multi, None, Some(4)).await?;
    let evidence = fetch_flow_admission_columns(handle.pool(), &multi).await?;
    assert_eq!(
        evidence.flow_capture_version, None,
        "a first-sighted request with more than one message must not admit — \
         mike may have joined the conversation mid-stream"
    );

    let unknown = uuid::Uuid::new_v4().to_string();
    resolve_session_id(handle.pool(), &unknown, None, None).await?;
    let evidence = fetch_flow_admission_columns(handle.pool(), &unknown).await?;
    assert_eq!(evidence.flow_capture_version, None);

    let repeated = uuid::Uuid::new_v4().to_string();
    resolve_session_id(handle.pool(), &repeated, None, Some(1)).await?;
    resolve_session_id(handle.pool(), &repeated, None, Some(9)).await?;
    let evidence = fetch_flow_admission_columns(handle.pool(), &repeated).await?;
    assert_eq!(
        evidence.flow_capture_version,
        Some(1),
        "ON CONFLICT DO NOTHING must keep the first call's admission evidence"
    );

    Ok(())
}

#[tokio::test]
async fn resolve_or_create_agent_id_idempotent_same_id() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let name = format!("agent-{}", uuid::Uuid::new_v4());

    let id1 = resolve_or_create_agent_id(handle.pool(), &name).await?;
    assert!(id1 > 0, "agent id must be a positive bigint");

    let id2 = resolve_or_create_agent_id(handle.pool(), &name).await?;
    assert_eq!(
        id1, id2,
        "resolve_or_create_agent_id must return the same id on conflict"
    );

    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*)::bigint FROM agents.agents WHERE name = $1")
            .bind(&name)
            .fetch_one(handle.pool())
            .await?;
    assert_eq!(count, 1, "exactly one agents row for the name");

    Ok(())
}

#[tokio::test]
async fn resolve_agent_id_by_uuid_none_for_missing() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;

    let missing = resolve_agent_id_by_uuid(handle.pool(), uuid::Uuid::new_v4()).await?;
    assert!(missing.is_none(), "unknown agent uuid must resolve to None");

    let name = format!("uuid-agent-{}", uuid::Uuid::new_v4());
    let created_id = resolve_or_create_agent_id(handle.pool(), &name).await?;
    let (agent_uuid,): (uuid::Uuid,) =
        sqlx::query_as("SELECT uuid FROM agents.agents WHERE id = $1")
            .bind(created_id)
            .fetch_one(handle.pool())
            .await?;
    let resolved = resolve_agent_id_by_uuid(handle.pool(), agent_uuid).await?;
    assert_eq!(
        resolved,
        Some(created_id),
        "known agent uuid must resolve to its bigint id"
    );

    Ok(())
}

#[tokio::test]
async fn resolve_lesson_id_by_uuid_none_for_missing() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;

    let missing = resolve_lesson_id_by_uuid(handle.pool(), uuid::Uuid::new_v4()).await?;
    assert!(
        missing.is_none(),
        "unknown lesson uuid must resolve to None"
    );

    Ok(())
}
