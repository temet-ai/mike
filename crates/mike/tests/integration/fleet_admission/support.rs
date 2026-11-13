use mike::store::persistence::fleet_repo::FleetPersist as _;
use mike::store::persistence::launch_repo::LaunchPersist as _;
use std::path::Path;

use mike::config::ProxyConfig;
use mike::launch::fleet::FleetDefinition;
use mike::launch::fleet::events::{EventPayload, NewFleetEvent, SeatLaunchConfirmed};
use mike::state::AppState;
use mike::store::persistence::PersistHandle;
use mike::store::persistence::fleet::{FleetRow, NewFleet};
use mike::store::persistence::launch::{LaunchSurfaceRefs, NewLaunch, NewRun};
use mike::store::persistence::test_support::{
    TestDbHandle, acquire_ephemeral_db, seed_fleet_done, seed_fleet_starting,
};
use reqwest::{Method, StatusCode};
use serde_json::{Value, json};
use tempfile::TempDir;
use uuid::Uuid;

pub(super) struct Harness {
    pub(super) persist: PersistHandle,
    pub(super) fleet: Uuid,
    pub(super) run: String,
    pub(super) worker: Uuid,
    pub(super) other: Uuid,
    pub(super) acceptance: Uuid,
    pub(super) worker_launch: i64,
    pub(super) def: FleetDefinition,
    pub(super) repo: TempDir,
    base: String,
    client: reqwest::Client,
    db: TestDbHandle,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
}

impl Harness {
    pub(super) async fn boot() -> anyhow::Result<Option<Self>> {
        Self::boot_in_state("running").await
    }

    pub(super) async fn boot_in_state(initial_state: &str) -> anyhow::Result<Option<Self>> {
        let Some(db) = acquire_ephemeral_db().await? else {
            eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
            return Ok(None);
        };
        let persist = PersistHandle::connect(&db.database_url).await?;
        let repo = repository().await?;
        let sessions = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        for session in sessions {
            persist
                .launch()
                .ensure_session_exists(&session.to_string())
                .await?;
        }
        let [worker, other, acceptance] = sessions;
        let definition = definition(repo.path(), sessions);
        let run = format!("admission-{}", Uuid::new_v4());
        persist
            .launch()
            .upsert_run(&NewRun {
                run_id: run.clone(),
                project_path: Some(repo.path().display().to_string()),
                backend: "none".into(),
                workspace_ref: None,
                tab_ref: None,
            })
            .await?;
        let fleet = persist
            .fleet()
            .insert_fleet(&NewFleet {
                slug: run.clone(),
                project_path: repo.path().display().to_string(),
                definition: definition.clone(),
                missing: json!([]),
                created_by: None,
            })
            .await?
            .fleet_uuid;
        seed_fleet_starting(&persist, fleet, Some(&run)).await?;
        let mut launch = NewLaunch::new("none", repo.path().display().to_string());
        launch.run_id = Some(run.clone());
        launch.session_uuid = Some(worker);
        launch.seat = Some("worker-T1".into());
        let worker_launch = persist.launch().insert_launch(&launch).await?;
        persist
            .launch()
            .record_launch_opened(worker_launch, &LaunchSurfaceRefs::default())
            .await?;
        for (task, session) in [
            (Some("T1"), worker),
            (Some("T2"), other),
            (None, acceptance),
        ] {
            persist
                .fleet()
                .append_fleet_event(&NewFleetEvent {
                    run_id: run.clone(),
                    task: task.map(str::to_string),
                    session_uuid: Some(session),
                    payload: EventPayload::SeatLaunchConfirmed(SeatLaunchConfirmed {
                        launch_id: worker_launch,
                    }),
                })
                .await?;
        }
        seed_initial_state(&persist, fleet, &run, worker_launch, initial_state).await?;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let mut config = ProxyConfig::for_tests_in_memory();
        config.inspect_listen_addr = listener.local_addr()?.to_string();
        let mut state = AppState::new(config).await?;
        state.persist = Some(persist.clone());
        let router = mike::api::manifest::inspect_router(state);
        let base = format!("http://{}/api/fleets/{fleet}", listener.local_addr()?);
        let server = tokio::spawn(async move { axum::serve(listener, router).await });
        Ok(Some(Self {
            persist,
            fleet,
            run,
            worker,
            other,
            acceptance,
            worker_launch,
            def: FleetDefinition::from_value(&definition).map_err(anyhow::Error::msg)?,
            repo,
            base,
            client: reqwest::Client::new(),
            db,
            server,
        }))
    }

    pub(super) async fn request(
        &self,
        method: Method,
        route: &str,
        body: Value,
    ) -> anyhow::Result<(StatusCode, Value)> {
        let response = self
            .client
            .request(method, format!("{}{route}", self.base))
            .json(&body)
            .send()
            .await?;
        let status = response.status();
        Ok((status, response.json().await?))
    }

    pub(super) async fn state(&self, state: &str) -> anyhow::Result<FleetRow> {
        if state == "done" {
            self.persist
                .fleet()
                .record_fleet_launch_verified_exit(
                    self.worker_launch,
                    &self.run,
                    "synthetic fixture seat completed",
                )
                .await?;
            return Ok(seed_fleet_done(&self.persist, self.fleet).await?);
        }
        self.persist
            .fleet()
            .set_fleet_state(self.fleet, state, None, None)
            .await?;
        self.row().await
    }

    pub(super) async fn row(&self) -> anyhow::Result<FleetRow> {
        self.persist
            .fleet()
            .find_fleet(&self.fleet.to_string())
            .await?
            .ok_or_else(|| anyhow::anyhow!("fixture fleet vanished"))
    }

    pub(super) async fn inbox_len(&self, session: Uuid) -> anyhow::Result<usize> {
        Ok(self
            .persist
            .launch()
            .fetch_for_session_with_activity(session)
            .await?
            .inbox
            .len())
    }

    pub(super) async fn cleanup(self) -> anyhow::Result<()> {
        self.server.abort();
        let _ = self.server.await;
        self.db.cleanup().await
    }
}

async fn seed_initial_state(
    persist: &PersistHandle,
    fleet: Uuid,
    run: &str,
    launch: i64,
    state: &str,
) -> anyhow::Result<()> {
    match state {
        "starting" => Ok(()),
        "failed" | "ready" | "draft" => seed_failed_state(persist, fleet, run, launch, state).await,
        "running" | "stopping" | "done" => {
            seed_active_state(persist, fleet, run, launch, state).await
        }
        other => anyhow::bail!("unsupported admission fixture state {other}"),
    }
}

async fn seed_failed_state(
    persist: &PersistHandle,
    fleet: Uuid,
    run: &str,
    launch: i64,
    state: &str,
) -> anyhow::Result<()> {
    persist
        .fleet()
        .set_fleet_state(fleet, "failed", None, None)
        .await?;
    if state != "failed" {
        persist
            .fleet()
            .record_fleet_launch_verified_exit(launch, run, "synthetic failed-start seat completed")
            .await?;
        persist
            .fleet()
            .set_fleet_state(fleet, state, None, None)
            .await?;
    }
    Ok(())
}

async fn seed_active_state(
    persist: &PersistHandle,
    fleet: Uuid,
    run: &str,
    launch: i64,
    state: &str,
) -> anyhow::Result<()> {
    persist
        .fleet()
        .set_fleet_state(fleet, "running", None, None)
        .await?;
    if state != "running" {
        persist
            .fleet()
            .set_fleet_state(fleet, "stopping", None, None)
            .await?;
    }
    if state == "done" {
        persist
            .fleet()
            .record_fleet_launch_verified_exit(launch, run, "synthetic fixture seat completed")
            .await?;
        seed_fleet_done(persist, fleet).await?;
    }
    Ok(())
}

pub(super) async fn git(repo: &Path, args: &[&str]) -> anyhow::Result<String> {
    let output = tokio::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .await?;
    anyhow::ensure!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

async fn repository() -> anyhow::Result<TempDir> {
    let repo = tempfile::tempdir()?;
    git(repo.path(), &["init", "-q", "-b", "work"]).await?;
    git(repo.path(), &["config", "user.name", "Admission Test"]).await?;
    git(
        repo.path(),
        &["config", "user.email", "admission@example.test"],
    )
    .await?;
    git(repo.path(), &["config", "commit.gpgsign", "false"]).await?;
    std::fs::create_dir(repo.path().join("src"))?;
    for name in ["owned", "extra", "other"] {
        std::fs::write(
            repo.path().join(format!("src/{name}.rs")),
            format!("pub fn {name}() {{}}\n"),
        )?;
    }
    git(repo.path(), &["add", "src"]).await?;
    git(repo.path(), &["commit", "-qm", "fixture"]).await?;
    Ok(repo)
}

fn definition(repo: &Path, [worker, other, acceptance]: [Uuid; 3]) -> Value {
    json!({
        "repo": repo,
        "branch": "work",
        "goal": "Keep history available without starting stopped work",
        "criteria": {"C1": {"text": "owned"}, "C2": {"text": "other"}},
        "coordinator": {"session": acceptance},
        "observer": {"session": acceptance},
        "acceptance": {"session": acceptance},
        "workers": {"rust": {"agent": "rust"}},
        "tasks": {
            "T1": {"name": "owned", "discipline": "rust", "session": worker,
                "seat": "worker-T1", "files": ["src/owned.rs", "src/extra.rs"],
                "criteria": ["C1"]},
            "T2": {"name": "other", "discipline": "rust", "session": other,
                "seat": "worker-T2", "files": ["src/other.rs"], "criteria": ["C2"]}
        },
        "check": {"command": "printf check > admission-spawned", "paths": ["src/"]},
        "gate": {"command": "printf gate > admission-spawned"}
    })
}
