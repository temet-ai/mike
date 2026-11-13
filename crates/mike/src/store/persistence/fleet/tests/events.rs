use super::fixture;
use super::seed::seed_run;
use crate::launch::fleet::events::{
    CommitLanded, EventKind, EventPayload, NewFleetEvent, StopRequested,
};
use crate::store::persistence::fleet_repo::FleetPersist as _;

fn event(run: &str, task: Option<&str>, payload: EventPayload) -> NewFleetEvent {
    NewFleetEvent {
        run_id: run.to_owned(),
        task: task.map(str::to_owned),
        session_uuid: None,
        payload,
    }
}

#[tokio::test]
async fn events_come_back_in_append_order_after_a_cursor_and_per_task() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run = "events-run-a";
    seed_run(&f.handle, run).await?;
    let first = f
        .handle
        .fleet()
        .append_fleet_event(&event(
            run,
            Some("T3"),
            EventPayload::CommitLanded(CommitLanded { sha: "a1".into() }),
        ))
        .await?;
    let second = f
        .handle
        .fleet()
        .append_fleet_event(&event(
            run,
            Some("T4"),
            EventPayload::CommitLanded(CommitLanded { sha: "b2".into() }),
        ))
        .await?;
    let third = f
        .handle
        .fleet()
        .append_fleet_event(&event(
            run,
            None,
            EventPayload::StopRequested(StopRequested { by: "op".into() }),
        ))
        .await?;
    assert!(first < second && second < third);

    let all = f.handle.fleet().fleet_events_since(run, 0).await?;
    assert_eq!(
        all.iter().map(|e| e.id).collect::<Vec<_>>(),
        [first, second, third]
    );
    assert_eq!(all[2].kind, EventKind::StopRequested);
    assert_eq!(all[2].task, None);

    let after = f.handle.fleet().fleet_events_since(run, first).await?;
    assert_eq!(
        after.iter().map(|e| e.id).collect::<Vec<_>>(),
        [second, third]
    );

    let t3 = f.handle.fleet().fleet_events_for_task(run, "T3").await?;
    assert_eq!(t3.len(), 1);
    assert_eq!(
        t3[0].decode()?,
        EventPayload::CommitLanded(CommitLanded { sha: "a1".into() })
    );
    assert!(
        f.handle
            .fleet()
            .fleet_events_since("other-run", 0)
            .await?
            .is_empty()
    );
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn an_event_for_a_run_that_does_not_exist_is_refused() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    seed_run(&f.handle, "events-run-b").await?;
    let refused = f
        .handle
        .fleet()
        .append_fleet_event(&event(
            "no-such-run",
            None,
            EventPayload::StopRequested(StopRequested { by: "op".into() }),
        ))
        .await;
    assert!(refused.is_err());
    f.db.cleanup().await?;
    Ok(())
}
