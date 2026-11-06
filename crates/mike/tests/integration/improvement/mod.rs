mod actions;
mod approve;
mod approve_http;
mod effect;

use mike::store::persistence::PersistHandle;
use mike::store::test_support::{self, TestDb, TestDbHandle};
use sqlx::PgPool;

pub(crate) struct Fixture {
    #[expect(
        dead_code,
        reason = "held only to keep the ephemeral test database alive for the fixture's lifetime"
    )]
    pub(crate) db: TestDbHandle,
    pub(crate) handle: PersistHandle,
    pub(crate) project_id: i64,
    pub(crate) project_path: String,
}

pub(crate) async fn fixture(project_path: &str) -> anyhow::Result<Option<Fixture>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping improvement test");
        return Ok(None);
    };
    let db = test_db.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project_id = test_support::seed_project(handle.pool(), project_path).await?;
    Ok(Some(Fixture {
        db,
        handle,
        project_id,
        project_path: project_path.to_owned(),
    }))
}

pub(crate) async fn open_directive_proposal(pool: &PgPool, project_id: i64) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.directive_proposals
            (source, scope, project_id, class, polarity, proposed_kind, proposed_text,
             evidence_count, session_count, first_evidence_at, last_evidence_at, window_days,
             rollup_id, proposer_version)
         VALUES ('repeated_directive', 'project', $1, 'commit', 'allow', 'correction',
                 'commit only when the tests are green', 1, 1, now(), now(), 30, 1, 'test-v1')
         RETURNING id",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub(crate) async fn proposal_status(pool: &PgPool, id: i64) -> anyhow::Result<String> {
    Ok(
        sqlx::query_scalar("SELECT status FROM memory.directive_proposals WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await?,
    )
}

pub(crate) async fn lesson_stage_and_source(
    pool: &PgPool,
    lesson_id: i64,
) -> anyhow::Result<(String, String)> {
    let row: (String, String) = sqlx::query_as(
        "SELECT ls.code, lsrc.code FROM memory.lessons l
           JOIN memory.lesson_stages ls ON ls.id = l.stage_id
           JOIN memory.lesson_sources lsrc ON lsrc.id = l.source_id
          WHERE l.id = $1",
    )
    .bind(lesson_id)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

mod migration;
