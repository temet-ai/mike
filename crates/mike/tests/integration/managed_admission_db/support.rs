use std::path::Path;
use std::time::Duration;

use mike::launch::fleet::test_seams::durable::{self, Domain, Request};
use mike::store::persistence::PersistHandle;
use mike::store::persistence::fleet::{FleetRow, NewFleet};
use mike::store::persistence::launch::NewRun;
use mike::store::persistence::test_support::{
    TestDbHandle, acquire_ephemeral_db, seed_fleet_running,
};
use serde_json::json;
use uuid::Uuid;

pub(super) struct Harness {
    pub persist: PersistHandle,
    pub other: PersistHandle,
    pub fleet: FleetRow,
    pub run: String,
    pub repo: tempfile::TempDir,
    pub domain: Domain,
    db: TestDbHandle,
}

impl Harness {
    pub(super) async fn new() -> anyhow::Result<Option<Self>> {
        let Some(db) = acquire_ephemeral_db().await? else {
            eprintln!("MIKE_TEST_ADMIN_URL unset; skipping managed admission DB test");
            return Ok(None);
        };
        let persist = PersistHandle::connect(&db.database_url).await?;
        let other = PersistHandle::connect(&db.database_url).await?;
        let repo = tempfile::tempdir()?;
        let run = format!("managed-db-{}", Uuid::new_v4());
        persist
            .upsert_run(&NewRun {
                run_id: run.clone(),
                project_path: Some(repo.path().display().to_string()),
                backend: "none".into(),
                workspace_ref: None,
                tab_ref: None,
            })
            .await?;
        let fleet = persist
            .insert_fleet(&NewFleet {
                slug: run.clone(),
                project_path: repo.path().display().to_string(),
                definition: json!({"retrospective":{"enabled":false}}),
                missing: json!([]),
                created_by: None,
            })
            .await?;
        let fleet = seed_fleet_running(&persist, fleet.fleet_uuid, &run).await?;
        Ok(Some(Self {
            persist,
            other,
            fleet,
            run,
            repo,
            domain: Domain::new()?,
            db,
        }))
    }

    pub(super) fn request(&self, command: &str) -> Request {
        Request {
            fleet: self.fleet.fleet_uuid,
            run: self.run.clone(),
            ledger: format!("{}#command:fixture", self.run),
            command: command.into(),
        }
    }

    pub(super) async fn stop_other(&self) -> anyhow::Result<FleetRow> {
        self.other
            .transition_fleet_state(&self.fleet, "stopping", Some(&self.run), None)
            .await?
            .ok_or_else(|| anyhow::anyhow!("fixture exact-run stop CAS lost"))
    }

    pub(super) async fn finish(self) -> anyhow::Result<()> {
        self.persist.pool().close().await;
        self.other.pool().close().await;
        self.db.cleanup().await
    }
}

pub(super) async fn phase(run: &str, wanted: &str) -> anyhow::Result<()> {
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if durable::snapshot(run)
                .pending
                .iter()
                .any(|entry| format!("{:?}", entry.phase) == wanted)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await?;
    Ok(())
}

pub(super) async fn file(path: &Path, text: &str) -> anyhow::Result<()> {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !std::fs::read_to_string(path).is_ok_and(|value| value.contains(text)) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    Ok(())
}

pub(super) fn command(h: &Harness) -> durable::Command {
    durable::Command {
        cwd: h.repo.path().into(),
        timeout: Duration::from_secs(30),
        log: None,
    }
}
