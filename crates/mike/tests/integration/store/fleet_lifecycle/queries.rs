use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::{fixture, opened_launch, running, starting, stopping};

#[tokio::test]
async fn unresolved_launches_include_all_251_rows_and_historical_unknown_exits()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run = format!("many-{}", Uuid::new_v4());
    running(&f.handle, &run).await?;
    let ids: Vec<i64> = sqlx::query_scalar(
        "INSERT INTO launch.launches (backend, folder, run_id, opened, closed_at, close_process_exited)
         SELECT 'herdr', '/tmp/lifecycle-many', $1, TRUE,
                CASE WHEN n % 3 = 0 THEN NULL ELSE now() END,
                CASE WHEN n % 3 = 1 THEN FALSE ELSE NULL END
           FROM generate_series(1, 251) AS seed(n)
         RETURNING id",
    ).bind(&run).fetch_all(f.handle.pool()).await?;
    sqlx::query(
        "INSERT INTO launch.launches (backend, folder, run_id, opened, closed_at, close_process_exited, result)
         VALUES ('herdr', '/tmp/lifecycle-many', $1, TRUE, now(), TRUE, 'success'),
                ('herdr', '/tmp/lifecycle-many', $1, FALSE, NULL, NULL, 'failed')",
    ).bind(&run).execute(f.handle.pool()).await?;
    let other = format!("other-{}", Uuid::new_v4());
    running(&f.handle, &other).await?;
    opened_launch(&f.handle, &other).await?;
    let unresolved = f.handle.unsettled_fleet_launches(&run).await?;
    assert_eq!(unresolved.len(), 251);
    assert_eq!(unresolved.iter().map(|row| row.id).collect::<Vec<_>>(), ids);
    assert!(
        unresolved
            .iter()
            .any(|row| row.closed_at.is_some() && row.close_process_exited == Some(false))
    );
    assert!(
        unresolved
            .iter()
            .any(|row| row.closed_at.is_some() && row.close_process_exited.is_none())
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn pending_commands_use_literal_run_namespace_and_ignore_finished_receipts()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run = format!("run_%_{}", Uuid::new_v4());
    let own = vec![
        run.clone(),
        format!("{run}#gate:isolated-source-v2"),
        format!("{run}#test:one"),
        format!("{run}#command:abc"),
        format!("{run}#baseline:commit"),
        format!("{run}#ruling:one"),
    ];
    let mut all = own.clone();
    all.extend([
        format!("{run}-other#gate"),
        format!("prefix-{run}#gate"),
        format!("{}#gate", run.replace('%', "wild").replace('_', "x")),
        format!("{run}#finished"),
    ]);
    sqlx::query(
        "INSERT INTO launch.tree_checks (run_id, state, command, finished_at)
         SELECT ledger, 'state', 'test command',
                CASE WHEN ledger = $2 THEN now() ELSE NULL END
           FROM UNNEST($1::text[]) AS seed(ledger)",
    )
    .bind(&all)
    .bind(format!("{run}#finished"))
    .execute(f.handle.pool())
    .await?;
    let pending = f.handle.pending_fleet_commands(&run).await?;
    assert_eq!(
        pending
            .iter()
            .map(|row| row.run_id.clone())
            .collect::<Vec<_>>(),
        own
    );
    assert!(
        f.handle
            .pending_fleet_commands("missing-run")
            .await?
            .is_empty()
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn verified_exit_upgrades_false_without_moving_first_close_or_another_run()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run = format!("exit-{}", Uuid::new_v4());
    running(&f.handle, &run).await?;
    let id = opened_launch(&f.handle, &run).await?;
    let closed_at: DateTime<Utc> = sqlx::query_scalar(
        "UPDATE launch.launches SET closed_at = now(), close_process_exited = FALSE,
         close_reason = 'legacy unverified closure' WHERE id = $1 RETURNING closed_at",
    )
    .bind(id)
    .fetch_one(f.handle.pool())
    .await?;
    assert!(
        f.handle
            .record_fleet_launch_verified_exit(id, "wrong-run", "observed exit")
            .await
            .is_err()
    );
    assert_eq!(f.handle.unsettled_fleet_launches(&run).await?.len(), 1);
    f.handle
        .record_fleet_launch_verified_exit(id, &run, "observed exit")
        .await?;
    f.handle
        .record_fleet_launch_verified_exit(id, &run, "repeat observation")
        .await?;
    let row: (DateTime<Utc>, bool, String) = sqlx::query_as(
        "SELECT closed_at, close_process_exited, close_reason FROM launch.launches WHERE id = $1",
    )
    .bind(id)
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(row, (closed_at, true, "legacy unverified closure".into()));
    assert!(f.handle.unsettled_fleet_launches(&run).await?.is_empty());
    assert!(
        f.handle
            .record_fleet_launch_verified_exit(-1, &run, "absent")
            .await
            .is_err()
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn admission_uses_exact_run_and_rejects_ambiguous_owner() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run = format!("admit-{}", Uuid::new_v4());
    let first = starting(&f.handle, Some(&run)).await?;
    let admitted = f
        .handle
        .fleet_run_admission(&run)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no fleet owner"))?;
    assert!(admitted.admits_work());
    assert_eq!(admitted.fleet_uuid, first.fleet_uuid);
    assert!(
        f.handle
            .fleet_run_admission(&format!("{run}#gate"))
            .await?
            .is_none()
    );
    stopping(&f.handle, &first).await?;
    let stopped = f
        .handle
        .fleet_run_admission(&run)
        .await?
        .ok_or_else(|| anyhow::anyhow!("stopping fleet lost ownership"))?;
    assert!(!stopped.admits_work());
    starting(&f.handle, Some(&run)).await?;
    assert!(f.handle.fleet_run_admission(&run).await.is_err());
    f.db.cleanup().await
}

#[tokio::test]
async fn only_verified_exit_releases_the_exact_launch_registration() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run = format!("registration-{}", Uuid::new_v4());
    running(&f.handle, &run).await?;
    let id = opened_launch(&f.handle, &run).await?;
    let session: Uuid =
        sqlx::query_scalar("SELECT session_uuid FROM launch.launches WHERE id = $1")
            .bind(id)
            .fetch_one(f.handle.pool())
            .await?;
    f.handle.ensure_session_exists(&session.to_string()).await?;
    sqlx::query(
        "INSERT INTO workspace.session_registrations
             (session_id, project_id, cwd, registered_via, lease_expires_at)
         SELECT id, project_id, '/tmp/fleet-lifecycle', 'mcp', now() + interval '15 minutes'
           FROM sessions.sessions WHERE session_uuid = $1",
    )
    .bind(session)
    .execute(f.handle.pool())
    .await?;
    assert!(
        f.handle
            .record_fleet_launch_verified_exit(id, "another-run", "wrong owner")
            .await
            .is_err()
    );
    let released: bool = sqlx::query_scalar(
        "SELECT r.released_at IS NOT NULL FROM workspace.session_registrations r
         JOIN sessions.sessions s ON s.id = r.session_id WHERE s.session_uuid = $1",
    )
    .bind(session)
    .fetch_one(f.handle.pool())
    .await?;
    assert!(!released);
    f.handle
        .record_fleet_launch_verified_exit(id, &run, "observed exit")
        .await?;
    let released: bool = sqlx::query_scalar(
        "SELECT r.released_at IS NOT NULL FROM workspace.session_registrations r
         JOIN sessions.sessions s ON s.id = r.session_id WHERE s.session_uuid = $1",
    )
    .bind(session)
    .fetch_one(f.handle.pool())
    .await?;
    assert!(released);
    f.db.cleanup().await
}
