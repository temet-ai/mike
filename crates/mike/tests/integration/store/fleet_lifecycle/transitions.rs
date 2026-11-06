use mike::store::persistence::PersistError;
use serde_json::json;
use uuid::Uuid;

use super::{fixture, opened_launch, running, starting, stopping};

#[tokio::test]
async fn stopping_rejects_stale_startup_and_preserves_timestamps() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run = format!("run-{}", Uuid::new_v4());
    let start = starting(&f.handle, Some(&run)).await?;
    let stopped = stopping(&f.handle, &start).await?;
    assert_eq!(stopped.state, "stopping");
    assert_eq!(stopped.started_at, start.started_at);
    assert_eq!(stopped.finished_at, start.finished_at);
    assert!(
        f.handle
            .transition_fleet_state(&start, "running", None, None)
            .await?
            .is_none()
    );
    assert!(
        f.handle
            .transition_fleet_state(&start, "failed", None, None)
            .await?
            .is_none()
    );
    for target in ["draft", "ready", "starting", "running", "failed", "done"] {
        let result = f
            .handle
            .set_fleet_state(start.fleet_uuid, target, None, None)
            .await;
        assert!(
            matches!(result, Err(PersistError::DataInvariant(_))),
            "{target}"
        );
    }
    let same = f
        .handle
        .set_fleet_state(start.fleet_uuid, "stopping", None, None)
        .await?;
    assert_eq!(same, stopped);
    let fresh = f.handle.find_fleet(&start.fleet_uuid.to_string()).await?;
    assert_eq!(fresh, Some(stopped));
    f.db.cleanup().await
}

#[tokio::test]
async fn transition_compares_run_identity_and_returns_current_row() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let unassociated = starting(&f.handle, None).await?;
    let run = format!("run-{}", Uuid::new_v4());
    f.handle
        .upsert_run(&mike::store::persistence::launch::NewRun {
            run_id: run.clone(),
            project_path: None,
            backend: "herdr".into(),
            workspace_ref: None,
            tab_ref: None,
        })
        .await?;
    let report = json!({"seat": "coordinator", "opened": true});
    let associated = f
        .handle
        .transition_fleet_state(&unassociated, "starting", Some(&run), Some(&report))
        .await?
        .ok_or_else(|| anyhow::anyhow!("association lost"))?;
    assert_eq!(associated.run_id.as_deref(), Some(run.as_str()));
    assert_eq!(associated.start_report, Some(report));
    assert!(
        f.handle
            .transition_fleet_state(&unassociated, "running", None, None)
            .await?
            .is_none()
    );
    assert!(
        f.handle
            .set_fleet_state(associated.fleet_uuid, "starting", Some("other"), None)
            .await
            .is_err()
    );
    let same = f
        .handle
        .set_fleet_state(associated.fleet_uuid, "starting", None, None)
        .await?;
    assert_eq!(same, associated);
    f.db.cleanup().await
}

#[tokio::test]
async fn missing_fleet_and_illegal_transitions_do_not_mutate_storage() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let start = starting(&f.handle, None).await?;
    for target in ["draft", "ready", "done", "paused"] {
        assert!(
            f.handle
                .set_fleet_state(start.fleet_uuid, target, None, None)
                .await
                .is_err()
        );
    }
    let mut missing = start.clone();
    missing.fleet_uuid = Uuid::new_v4();
    assert!(
        f.handle
            .transition_fleet_state(&missing, "stopping", None, None)
            .await?
            .is_none()
    );
    assert!(
        f.handle
            .set_fleet_state(missing.fleet_uuid, "ready", None, None)
            .await
            .is_err()
    );
    assert_eq!(
        f.handle.find_fleet(&start.fleet_uuid.to_string()).await?,
        Some(start)
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn concurrent_startup_and_stop_cannot_escape_stopping() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let start = starting(&f.handle, None).await?;
    let (running, stopped) = tokio::join!(
        f.handle
            .transition_fleet_state(&start, "running", None, None),
        f.handle
            .transition_fleet_state(&start, "stopping", None, None),
    );
    assert!(running?.is_some() ^ stopped?.is_some());
    let fresh = f
        .handle
        .find_fleet(&start.fleet_uuid.to_string())
        .await?
        .ok_or_else(|| anyhow::anyhow!("fleet missing"))?;
    let stopped = if fresh.state == "running" {
        stopping(&f.handle, &fresh).await?
    } else {
        fresh
    };
    assert_eq!(stopped.state, "stopping");
    assert!(
        f.handle
            .transition_fleet_state(&start, "running", None, None)
            .await?
            .is_none()
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn failed_retry_requires_settled_launches_and_commands() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run = format!("failed-{}", Uuid::new_v4());
    let start = starting(&f.handle, Some(&run)).await?;
    let launch = opened_launch(&f.handle, &run).await?;
    let receipt = f
        .handle
        .begin_tree_check(&run, "state", "true", "test")
        .await?;
    let failed = f
        .handle
        .transition_fleet_state(&start, "failed", None, None)
        .await?
        .ok_or_else(|| anyhow::anyhow!("failure transition lost"))?;
    assert!(
        f.handle
            .set_fleet_state(failed.fleet_uuid, "ready", None, None)
            .await
            .is_err()
    );
    f.handle
        .record_fleet_launch_verified_exit(launch, &run, "verified test exit")
        .await?;
    assert!(
        f.handle
            .set_fleet_state(failed.fleet_uuid, "draft", None, None)
            .await
            .is_err()
    );
    f.handle
        .finish_tree_check(receipt, None, "cancelled", None)
        .await?;
    let ready = f
        .handle
        .set_fleet_state(failed.fleet_uuid, "ready", None, None)
        .await?;
    assert_eq!(ready.state, "ready");
    f.db.cleanup().await
}

#[tokio::test]
async fn failed_start_with_live_launch_accepts_stopping_cleanup() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run = format!("failed-stop-{}", Uuid::new_v4());
    let start = starting(&f.handle, Some(&run)).await?;
    opened_launch(&f.handle, &run).await?;
    let failed = f
        .handle
        .transition_fleet_state(&start, "failed", None, None)
        .await?
        .ok_or_else(|| anyhow::anyhow!("failure transition lost"))?;
    assert_eq!(stopping(&f.handle, &failed).await?.state, "stopping");
    f.db.cleanup().await
}

#[tokio::test]
async fn running_cannot_regress_and_no_payload_retry_is_timestamp_stable() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run = format!("running-{}", Uuid::new_v4());
    let row = running(&f.handle, &run).await?;
    for state in ["draft", "ready", "starting", "failed", "done"] {
        assert!(
            f.handle
                .set_fleet_state(row.fleet_uuid, state, None, None)
                .await
                .is_err()
        );
    }
    assert_eq!(
        f.handle
            .set_fleet_state(row.fleet_uuid, "running", None, None)
            .await?,
        row
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn same_state_and_run_with_changed_definition_rejects_stale_revision() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run = format!("revision-{}", Uuid::new_v4());
    let row = running(&f.handle, &run).await?;
    f.handle
        .save_fleet_definition(
            row.fleet_uuid,
            &json!({"edited": true}),
            &row.missing,
            row.updated_at,
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("definition save lost"))?;
    let fresh = f
        .handle
        .find_fleet(&row.fleet_uuid.to_string())
        .await?
        .ok_or_else(|| anyhow::anyhow!("saved fleet missing"))?;
    assert_eq!(fresh.state, row.state);
    assert_eq!(fresh.run_id, row.run_id);
    assert!(
        f.handle
            .transition_fleet_state(&row, "stopping", None, None)
            .await?
            .is_none()
    );
    assert_eq!(
        stopping(&f.handle, &fresh).await?.definition,
        json!({"edited": true})
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn draft_ready_draft_aba_does_not_accept_original_revision() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let row = f
        .handle
        .insert_fleet(&mike::store::persistence::fleet::NewFleet {
            slug: format!("aba-{}", Uuid::new_v4()),
            project_path: "/tmp/aba".into(),
            definition: json!({}),
            missing: json!([]),
            created_by: None,
        })
        .await?;
    let ready = f
        .handle
        .transition_fleet_state(&row, "ready", None, None)
        .await?
        .ok_or_else(|| anyhow::anyhow!("ready transition lost"))?;
    let draft = f
        .handle
        .transition_fleet_state(&ready, "draft", None, None)
        .await?
        .ok_or_else(|| anyhow::anyhow!("draft transition lost"))?;
    assert_eq!(draft.state, row.state);
    assert_eq!(draft.run_id, row.run_id);
    assert!(
        f.handle
            .transition_fleet_state(&row, "ready", None, None)
            .await?
            .is_none()
    );
    assert!(
        f.handle
            .transition_fleet_state(&draft, "ready", None, None)
            .await?
            .is_some()
    );
    f.db.cleanup().await
}
