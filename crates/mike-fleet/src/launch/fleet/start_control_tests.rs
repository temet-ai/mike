use serde_json::json;
use uuid::Uuid;

use super::{control_patch, control_seats, fail, notebook_problem};
use crate::launch::fleet::definition::merge;
use crate::launch::fleet::plan::ControlSeats;
use crate::launch::fleet::test_fixtures::{changed, parsed};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::NewFleet;
use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::test_support::acquire_ephemeral_db;

async fn session(persist: &PersistHandle) -> anyhow::Result<Uuid> {
    let id = Uuid::new_v4();
    persist
        .launch()
        .ensure_session_exists(&id.to_string())
        .await?;
    Ok(id)
}

#[tokio::test]
async fn an_undeclared_coordinator_is_the_author_and_no_session_id_is_invented()
-> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let author = session(&persist).await?;
    let def = changed(json!({"coordinator": {"session": null}, "acceptance": {"session": null}}));
    let seats = control_seats(&persist, &def, Some(author)).await;
    assert_eq!(seats, ControlSeats::answering(Some(author), None));
    let alone = control_seats(&persist, &def, None).await;
    assert_eq!(alone, ControlSeats::answering(None, None));
    db.cleanup().await
}

#[tokio::test]
async fn a_recorded_session_that_never_existed_is_dropped_and_a_real_one_is_kept()
-> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let (author, running) = (session(&persist).await?, session(&persist).await?);
    let phantom = Uuid::new_v4();
    let stale = changed(json!({
        "coordinator": {"session": phantom},
        "acceptance": {"session": phantom},
    }));
    assert_eq!(
        control_seats(&persist, &stale, Some(author)).await,
        ControlSeats::answering(Some(author), None)
    );
    let declared = changed(json!({
        "coordinator": {"session": running},
        "acceptance": {"session": running},
    }));
    assert_eq!(
        control_seats(&persist, &declared, Some(author)).await,
        ControlSeats::answering(Some(running), Some(running))
    );
    db.cleanup().await
}

#[test]
fn the_patch_for_seats_nobody_runs_removes_the_recorded_ids() {
    let mut value = json!({
        "coordinator": {"session": Uuid::new_v4(), "model": "opus"},
        "acceptance": {"session": Uuid::new_v4()},
    });
    merge(
        &mut value,
        &control_patch(&ControlSeats::answering(None, None)),
    );
    let def = parsed(&value);
    assert_eq!(def.coordinator.session, None);
    assert_eq!(def.acceptance.session, None);
    assert_eq!(def.coordinator.model, "opus");
}

#[test]
fn briefs_kept_as_notes_need_a_session_to_hold_them() {
    let def = changed(json!({"briefs": {"as_notes": true}}));
    let problem = notebook_problem(&def, &ControlSeats::answering(None, None));
    assert!(problem.is_some_and(|text| text.contains("coordinator.session")));
    let held = ControlSeats::answering(Some(Uuid::new_v4()), None);
    assert_eq!(notebook_problem(&def, &held), None);
    let inline = changed(json!({}));
    assert_eq!(
        notebook_problem(&inline, &ControlSeats::answering(None, None)),
        None
    );
}

#[tokio::test]
async fn a_failed_start_mails_the_session_that_created_the_fleet() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let author = session(&persist).await?;
    let slug = format!("start-fail-{}", Uuid::new_v4());
    let row = persist
        .fleet()
        .insert_fleet(&NewFleet {
            slug: slug.clone(),
            project_path: "/tmp/start-fail".into(),
            definition: json!({}),
            missing: json!([]),
            created_by: Some(author),
        })
        .await?;
    let run = format!("fleet-{slug}");
    let ready = persist
        .fleet()
        .transition_fleet_state(&row, "ready", None, None)
        .await?
        .ok_or_else(|| anyhow::anyhow!("fleet did not become ready"))?;
    persist
        .fleet()
        .transition_fleet_state(&ready, "starting", Some(&run), None)
        .await?
        .ok_or_else(|| anyhow::anyhow!("fleet did not start"))?;
    fail(&persist, row.fleet_uuid, "the database is not reachable").await;
    let failed = persist
        .fleet()
        .find_fleet(&row.fleet_uuid.to_string())
        .await?;
    assert_eq!(failed.map(|f| f.state), Some("failed".to_string()));
    let inbox = persist
        .launch()
        .fetch_for_session_with_activity(author)
        .await?
        .inbox;
    assert_eq!(inbox.len(), 1);
    assert!(
        inbox[0]
            .row
            .body
            .contains(&format!("Fleet {slug} did not start"))
    );
    db.cleanup().await
}
