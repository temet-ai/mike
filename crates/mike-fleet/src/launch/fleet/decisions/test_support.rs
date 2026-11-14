use serde_json::{Value, json};
use uuid::Uuid;

use crate::launch::fleet::definition::merge;
use crate::launch::fleet::events::{EventPayload, NewFleetEvent, SeatLaunchConfirmed};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::NewFleet;
use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch::{NewLaunch, NewRun};
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::test_support::{
    TestDbHandle, acquire_ephemeral_db, seed_fleet_running,
};

pub struct DecisionsFixture {
    pub db: TestDbHandle,
    pub handle: PersistHandle,
    pub run: String,
    pub fleet_id: i64,
    pub coordinator: Uuid,
    pub worker: Uuid,
    pub peer: Uuid,
}

async fn launch(
    handle: &PersistHandle,
    run: &str,
    session: Uuid,
    seat: &str,
) -> anyhow::Result<i64> {
    handle
        .launch()
        .ensure_session_exists(&session.to_string())
        .await?;
    let mut new = NewLaunch::new("herdr", "/tmp/decisions-test");
    new.run_id = Some(run.to_owned());
    new.session_uuid = Some(session);
    new.seat = Some(seat.to_owned());
    Ok(handle.launch().insert_launch(&new).await?)
}

fn definition(extra: &Value, (coordinator, worker, peer): (Uuid, Uuid, Uuid)) -> Value {
    let mut base = json!({
        "coordinator": {"session": coordinator.to_string()},
        "tasks": {
            "T1": {"name": "Parser", "discipline": "backend", "files": ["src/parser/"],
                   "seat": "worker-T1", "session": worker.to_string()},
            "T2": {"name": "Docs", "discipline": "docs", "files": ["docs/"],
                   "seat": "worker-T2", "session": peer.to_string()}
        }
    });
    merge(&mut base, extra);
    base
}

impl DecisionsFixture {
    pub async fn boot(extra: Value) -> anyhow::Result<Option<Self>> {
        let Some(db) = acquire_ephemeral_db().await? else {
            eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
            return Ok(None);
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let run = format!("decisions-{}", Uuid::new_v4());
        handle
            .launch()
            .upsert_run(&NewRun {
                run_id: run.clone(),
                project_path: None,
                backend: "herdr".into(),
                workspace_ref: None,
                tab_ref: None,
            })
            .await?;
        let (coordinator, worker, peer) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        launch(&handle, &run, coordinator, "coordinator").await?;
        let launch_id = launch(&handle, &run, worker, "worker-T1").await?;
        let peer_launch = launch(&handle, &run, peer, "worker-T2").await?;
        let fleet = handle
            .fleet()
            .insert_fleet(&NewFleet {
                slug: run.clone(),
                project_path: "/tmp/decisions-test".into(),
                definition: definition(&extra, (coordinator, worker, peer)),
                missing: json!([]),
                created_by: None,
            })
            .await?;
        let row = seed_fleet_running(&handle, fleet.fleet_uuid, &run).await?;
        for (task, session, id) in [("T1", worker, launch_id), ("T2", peer, peer_launch)] {
            handle
                .fleet()
                .append_fleet_event(&NewFleetEvent {
                    run_id: run.clone(),
                    task: Some(task.into()),
                    session_uuid: Some(session),
                    payload: EventPayload::SeatLaunchConfirmed(SeatLaunchConfirmed {
                        launch_id: id,
                    }),
                })
                .await?;
        }
        Ok(Some(Self {
            db,
            handle,
            run,
            fleet_id: row.id,
            coordinator,
            worker,
            peer,
        }))
    }

    pub async fn finish(self) -> anyhow::Result<()> {
        self.handle.pool().close().await;
        self.db.cleanup().await
    }
}
