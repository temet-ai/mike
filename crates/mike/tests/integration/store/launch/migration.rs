use uuid::Uuid;

use mike::store::persistence::PersistHandle;

use crate::test_db::TestDb;

const LAUNCH_DOMAIN_MIGRATION: &str = include_str!(
    "../../../../../mike-base/src/store/persistence/migrations/20260916000001_launch_domain.sql"
);

#[tokio::test]
async fn launch_migration_applied_twice_converges() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;

    sqlx::raw_sql(LAUNCH_DOMAIN_MIGRATION)
        .execute(handle.pool())
        .await?;

    let checks: Vec<String> = sqlx::query_scalar(
        "SELECT conname::text FROM pg_constraint \
         WHERE conrelid = 'workspace.session_registrations'::regclass AND contype = 'c' \
         AND pg_get_constraintdef(oid) LIKE '%registered_via%' ORDER BY 1",
    )
    .fetch_all(handle.pool())
    .await?;
    assert_eq!(
        checks,
        vec!["session_registrations_registered_via_check".to_owned()],
        "exactly one CHECK governs registered_via after a re-run"
    );

    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name::text FROM information_schema.tables \
         WHERE table_schema = 'launch' ORDER BY 1",
    )
    .fetch_all(handle.pool())
    .await?;
    assert_eq!(
        tables,
        vec![
            "backend_validations",
            "fleet_events",
            "fleets",
            "introductions",
            "launches",
            "run_records",
            "runs",
            "templates",
            "told",
            "tree_changes",
            "tree_checks"
        ]
    );

    Ok(())
}

#[tokio::test]
async fn registered_via_accepts_launcher_and_rejects_unknown() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let session = Uuid::new_v4();
    sqlx::query(
        "WITH p AS (INSERT INTO projects.projects (project_path, display_name) \
                    VALUES ($2, $2) RETURNING id) \
         INSERT INTO sessions.sessions (session_uuid, project_id) SELECT $1, id FROM p",
    )
    .bind(session)
    .bind(format!("/tmp/launcher-reg-{}", Uuid::new_v4()))
    .execute(handle.pool())
    .await?;

    let insert = "INSERT INTO workspace.session_registrations \
                  (session_id, project_id, cwd, registered_via, lease_expires_at) \
                  SELECT s.id, s.project_id, '/tmp/launcher-reg', $2, now() + interval '5 minutes' \
                  FROM sessions.sessions s WHERE s.session_uuid = $1";
    sqlx::query(insert)
        .bind(session)
        .bind("launcher")
        .execute(handle.pool())
        .await?;

    let other = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sessions.sessions (session_uuid, project_id) \
         SELECT $1, project_id FROM sessions.sessions WHERE session_uuid = $2",
    )
    .bind(other)
    .bind(session)
    .execute(handle.pool())
    .await?;
    let refused = sqlx::query(insert)
        .bind(other)
        .bind("carrier-pigeon")
        .execute(handle.pool())
        .await;
    assert!(
        refused.is_err(),
        "a value outside the CHECK set must be refused"
    );

    Ok(())
}
