use uuid::Uuid;

use mike::store::persistence::PersistHandle;
use mike::store::test_support::{self, TestDb, TestDbHandle, acquire_ephemeral_db};

pub async fn shared_handle() -> anyhow::Result<Option<PersistHandle>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = test_db.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    Ok(Some(handle))
}

pub fn unique(label: &str) -> String {
    format!("{label}-{}", Uuid::new_v4().simple())
}

pub struct OpenTurnFixture {
    pub db: TestDbHandle,
    pub handle: PersistHandle,
    pub pool: sqlx::PgPool,
    pub session: String,
    pub turn_id: i64,
    pub request_id: i64,
}

pub async fn seed_open_turn_fixture() -> anyhow::Result<Option<OpenTurnFixture>> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(None);
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    let project_id = test_support::seed_project(&pool, &unique("w120-open-turn")).await?;
    let session = Uuid::new_v4().to_string();
    let spare = Uuid::new_v4().to_string();
    test_support::seed_two_sessions(&pool, &session, &spare, project_id).await?;
    test_support::seed_session_turns(&pool, &session, project_id, 1).await?;

    let (turn_id, request_id): (i64, i64) = sqlx::query_as(
        "UPDATE memory.operator_turns t
            SET closed_at = NULL
           FROM sessions.sessions s
          WHERE t.session_id = s.id AND s.session_uuid = $1::text::uuid
      RETURNING t.id, t.end_request_id",
    )
    .bind(&session)
    .fetch_one(&pool)
    .await?;

    Ok(Some(OpenTurnFixture {
        db,
        handle,
        pool,
        session,
        turn_id,
        request_id,
    }))
}
