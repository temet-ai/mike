use super::{events_in, fixture, repo, seed_session};
use crate::change_record::ChangeKind;
use crate::change_record::writers::change_record_enabled;
use crate::launch::fleet::check_report::{Finished, record_finished};
use crate::launch::fleet::definition::{FleetDefinition, TaskDefinition};
use crate::launch::fleet::events::{EventPayload, NewFleetEvent, SeatLaunchConfirmed};
use crate::store::persistence::launch::NewRun;

#[tokio::test]
async fn change_record_enabled_is_off_until_the_operator_turns_it_on() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    assert!(!change_record_enabled(&f.handle).await);
    f.handle
        .upsert_feature_toggle("change_record", true)
        .await?;
    assert!(change_record_enabled(&f.handle).await);
    f.handle
        .upsert_feature_toggle("change_record", false)
        .await?;
    assert!(!change_record_enabled(&f.handle).await);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn change_record_enabled_database_error_reads_as_off() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    f.handle
        .upsert_feature_toggle("change_record", true)
        .await?;
    f.handle.pool().close().await;
    assert!(!change_record_enabled(&f.handle).await);
    Ok(())
}

#[tokio::test]
async fn finished_check_and_gate_write_one_event_each_only_with_the_switch_on() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let repo = repo()?;
    let seeded = seed_session(&f.handle, "/p/finished", repo.path()).await?;
    let mut def = FleetDefinition {
        repo: repo.checkout(),
        ..Default::default()
    };
    def.tasks.insert("T1".into(), TaskDefinition::default());
    f.handle
        .upsert_run(&NewRun {
            run_id: "run-9".into(),
            project_path: Some(repo.checkout()),
            backend: "herdr".into(),
            workspace_ref: None,
            tab_ref: None,
        })
        .await?;
    f.handle
        .append_fleet_event(&NewFleetEvent {
            run_id: "run-9".into(),
            task: Some("T1".into()),
            session_uuid: Some(seeded.session_uuid),
            payload: EventPayload::SeatLaunchConfirmed(SeatLaunchConfirmed { launch_id: 1 }),
        })
        .await?;
    let finished = |label: &'static str, passed: bool| Finished {
        label,
        run: "run-9",
        command: "make check",
        commit: "0123456789abcdef",
        passed,
        source_verified: true,
        output: " --> src/lib.rs:3:1",
        root: None,
    };

    record_finished(&f.handle, &def, finished("check", false)).await;
    assert!(events_in(&f.handle, &repo.checkout()).await?.is_empty());

    f.handle
        .upsert_feature_toggle("change_record", true)
        .await?;
    record_finished(&f.handle, &def, finished("check", false)).await;
    record_finished(&f.handle, &def, finished("gate", true)).await;
    let events = events_in(&f.handle, &repo.checkout()).await?;
    assert_eq!(events.len(), 2);
    assert!(events.iter().all(|e| e.kind == ChangeKind::Check));
    assert!(events.iter().any(|e| e.subject.starts_with("gate passed")));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn finished_check_without_a_known_seat_session_writes_nothing() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let repo = repo()?;
    f.handle
        .upsert_feature_toggle("change_record", true)
        .await?;
    let def = FleetDefinition {
        repo: repo.checkout(),
        ..Default::default()
    };
    record_finished(
        &f.handle,
        &def,
        Finished {
            label: "check",
            run: "run-1",
            command: "c",
            commit: "abc",
            passed: true,
            source_verified: true,
            output: "",
            root: None,
        },
    )
    .await;
    assert!(events_in(&f.handle, &repo.checkout()).await?.is_empty());
    f.db.cleanup().await?;
    Ok(())
}
