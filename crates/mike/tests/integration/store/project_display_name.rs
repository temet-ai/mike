use mike::store::persistence::launch_repo::LaunchPersist as _;
use mike::store::persistence::{PersistHandle, SENTINEL_PROJECT_PATH, id_resolvers};
use mike::store::test_support::TestDb;

async fn seed_project_row(
    pool: &sqlx::PgPool,
    project_path: &str,
    display_name: &str,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $2, NOW(), NOW()) RETURNING id",
    )
    .bind(project_path)
    .bind(display_name)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

async fn fetch_display_name(pool: &sqlx::PgPool, project_id: i64) -> anyhow::Result<String> {
    let (display_name,): (String,) =
        sqlx::query_as("SELECT display_name FROM projects.projects WHERE id = $1")
            .bind(project_id)
            .fetch_one(pool)
            .await?;
    Ok(display_name)
}

async fn run_display_name_backfill(pool: &sqlx::PgPool, project_id: i64) -> anyhow::Result<u64> {
    let result = sqlx::query!(
        "WITH parsed AS (
             SELECT id,
                    regexp_split_to_array(
                        trim(both '/' FROM regexp_replace(project_path, '/+', '/', 'g')),
                        '/'
                    ) AS segs
               FROM projects.projects
              WHERE id = $1
         ),
         derived AS (
             SELECT id,
                    CASE
                        WHEN array_length(segs, 1) > 2
                            THEN array_to_string(segs[3:array_length(segs, 1)], '/')
                        ELSE array_to_string(segs, '/')
                    END AS derived_display_name
               FROM parsed
         )
         UPDATE projects.projects p
            SET display_name = derived.derived_display_name
           FROM derived
          WHERE p.id = derived.id
            AND p.display_name IS DISTINCT FROM derived.derived_display_name",
        project_id,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

#[tokio::test]
async fn pin_project_autocreate_derives_trimmed_display_name() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = test_db.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project_path = format!(
        "/Users/tester-{}/projects/temet/temet-proxy",
        uuid::Uuid::new_v4()
    );

    handle.launch().pin_project(&project_path).await?;

    let (display_name,): (String,) =
        sqlx::query_as("SELECT display_name FROM projects.projects WHERE project_path = $1")
            .bind(&project_path)
            .fetch_one(handle.pool())
            .await?;
    assert_ne!(
        display_name, project_path,
        "display_name must never equal the raw project_path for a >2-segment path"
    );
    assert_eq!(
        display_name, "projects/temet/temet-proxy",
        "display_name must drop the assumed /Users/<username> prefix"
    );

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn set_project_ttl_autocreate_derives_trimmed_display_name() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = test_db.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project_path = format!(
        "/home/tester-{}/work/deep/nested/repo",
        uuid::Uuid::new_v4()
    );

    handle
        .launch()
        .set_project_ttl(&project_path, Some(std::time::Duration::from_secs(3600)))
        .await?;

    let (display_name,): (String,) =
        sqlx::query_as("SELECT display_name FROM projects.projects WHERE project_path = $1")
            .bind(&project_path)
            .fetch_one(handle.pool())
            .await?;
    assert_ne!(
        display_name, project_path,
        "display_name must never equal the raw project_path for a >2-segment path"
    );
    assert_eq!(
        display_name, "work/deep/nested/repo",
        "display_name must drop the assumed /home/<username> prefix"
    );

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn project_display_name_backfill_fixes_buggy_rows_only() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = test_db.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let tester = uuid::Uuid::new_v4();

    let buggy_path = format!("/Users/backfill-tester-{tester}/projects/temet/temet-proxy");
    let buggy_id = seed_project_row(handle.pool(), &buggy_path, &buggy_path).await?;

    let correct_path = format!("/Users/backfill-tester-{tester}/projects/already/correct");
    let correct_id =
        seed_project_row(handle.pool(), &correct_path, "projects/already/correct").await?;

    let shallow_path = format!("no-project-{tester}");
    let shallow_id = seed_project_row(handle.pool(), &shallow_path, &shallow_path).await?;

    let first_run_rows = run_display_name_backfill(handle.pool(), buggy_id).await?;
    assert_eq!(first_run_rows, 1, "only the buggy row should be touched");
    let no_op_run_rows = run_display_name_backfill(handle.pool(), correct_id).await?;
    assert_eq!(
        no_op_run_rows, 0,
        "an already-correct row must be untouched"
    );
    let shallow_run_rows = run_display_name_backfill(handle.pool(), shallow_id).await?;
    assert_eq!(
        shallow_run_rows, 0,
        "a single-segment row stays a no-op, matching project_display_name's own threshold"
    );

    assert_eq!(
        fetch_display_name(handle.pool(), buggy_id).await?,
        "projects/temet/temet-proxy",
        "buggy row must be corrected to the trimmed form"
    );
    assert_eq!(
        fetch_display_name(handle.pool(), correct_id).await?,
        "projects/already/correct",
        "already-correct row must be untouched"
    );
    assert_eq!(
        fetch_display_name(handle.pool(), shallow_id).await?,
        shallow_path,
        "sentinel-shaped row (1 segment) stays a no-op, matching project_display_name's own threshold"
    );

    let second_run_rows = run_display_name_backfill(handle.pool(), buggy_id).await?;
    assert_eq!(
        second_run_rows, 0,
        "a second run must be a no-op — idempotent"
    );

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn resolve_session_id_sentinel_bootstrap_keeps_sentinel_display_name() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = test_db.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let session_id = uuid::Uuid::new_v4().to_string();

    id_resolvers::resolve_session_id(handle.pool(), &session_id, None, None).await?;

    let (display_name,): (String,) =
        sqlx::query_as("SELECT display_name FROM projects.projects WHERE project_path = $1")
            .bind(SENTINEL_PROJECT_PATH)
            .fetch_one(handle.pool())
            .await?;
    assert_eq!(
        display_name, SENTINEL_PROJECT_PATH,
        "sentinel row is exempt from the trim (1 segment, below the >2 threshold)"
    );

    handle.pool().close().await;
    Ok(())
}
