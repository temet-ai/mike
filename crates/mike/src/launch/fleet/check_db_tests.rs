#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use serde_json::json;
use uuid::Uuid;

use crate::launch::fleet::check::Verdict;
use crate::launch::fleet::check_report::tell_waiting_with;
use crate::launch::fleet::events::{EventPayload, NewFleetEvent, SeatLaunchConfirmed};
use crate::launch::fleet::tests::{complete, parsed};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::launch::NewRun;
use crate::store::persistence::test_support::acquire_ephemeral_db;

async fn inbox(persist: &PersistHandle, session: Uuid) -> anyhow::Result<Vec<String>> {
    Ok(persist
        .launch()
        .fetch_for_session_with_activity(session)
        .await?
        .inbox
        .into_iter()
        .map(|m| m.row.body)
        .collect())
}

fn mail_run() -> NewRun {
    NewRun {
        run_id: "fleet-mail-1".to_owned(),
        project_path: Some("/tmp/check-mail".to_owned()),
        backend: "herdr".to_owned(),
        workspace_ref: None,
        tab_ref: None,
    }
}

#[tokio::test]
async fn a_failed_check_is_mailed_to_each_of_three_askers_sorted_into_their_own_and_others_files()
-> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let fleet_db = persist.fleet();
    let launch_db = persist.launch();
    let sessions = [
        Uuid::from_u128(11),
        Uuid::from_u128(12),
        Uuid::from_u128(13),
    ];
    for session in sessions {
        launch_db
            .ensure_session_exists(&session.to_string())
            .await?;
    }
    let mut value = complete();
    for (task, seat, session) in [
        ("P1", "db-dev-P1", sessions[0]),
        ("P2", "frontend-developer-P2", sessions[1]),
        ("P10", "frontend-developer-P10", sessions[2]),
    ] {
        value["tasks"][task]["seat"] = json!(seat);
        value["tasks"][task]["session"] = json!(session.to_string());
    }
    let def = parsed(&value);
    launch_db.upsert_run(&mail_run()).await?;
    let id = fleet_db
        .begin_tree_check("fleet-mail-1", "state-a", "make check", "db-dev-P1")
        .await?;
    fleet_db
        .finish_tree_check(
            id,
            Some(1),
            "error\n  --> frontend/src/routes/improvement/page.svelte:9:1\n",
            Some("state-a"),
        )
        .await?;
    for seat in [
        "db-dev-P1",
        "frontend-developer-P2",
        "frontend-developer-P10",
    ] {
        fleet_db.add_tree_check_asker(id, seat).await?;
    }
    let row = fleet_db
        .latest_tree_check("fleet-mail-1")
        .await?
        .ok_or_else(|| anyhow::anyhow!("the finished check should be recorded"))?;

    let verdict = Verdict {
        row,
        commit: "0123456789abcdef".into(),
        how: "finished".into(),
    };
    for (task, session) in ["P1", "P2", "P10"].into_iter().zip(sessions) {
        fleet_db
            .append_fleet_event(&NewFleetEvent {
                run_id: "fleet-mail-1".to_owned(),
                task: Some(task.to_owned()),
                session_uuid: Some(session),
                payload: EventPayload::SeatLaunchConfirmed(SeatLaunchConfirmed { launch_id: 1 }),
            })
            .await?;
    }
    tell_waiting_with(&persist, &def, &verdict, sessions.to_vec()).await;

    let (first, owner, third) = (
        inbox(&persist, sessions[0]).await?,
        inbox(&persist, sessions[1]).await?,
        inbox(&persist, sessions[2]).await?,
    );
    assert_eq!(first.len(), 1, "{first:?}");
    assert_eq!(third.len(), 1, "{third:?}");
    assert_eq!(
        owner.len(),
        2,
        "its verdict and the one telling it its file fails: {owner:?}"
    );
    for body in [&first[0], &third[0]] {
        assert!(
            body.contains("failed at commit 0123456789") && body.contains("in other seats' files"),
            "{body}"
        );
        assert!(body.contains("page.svelte:9"), "{body}");
    }
    assert!(
        owner
            .iter()
            .any(|b| b.contains("in your files:") && b.contains("page.svelte:9")),
        "{owner:?}"
    );
    db.cleanup().await?;
    Ok(())
}
