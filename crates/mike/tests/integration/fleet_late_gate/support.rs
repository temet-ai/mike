use mike::launch::fleet::FleetDefinition;
use mike::launch::fleet::test_seams::gate_failures;
use mike::store::persistence::PersistHandle;
use mike::store::persistence::fleet::{FleetRow, NewFleet};
use mike::store::persistence::fleet_repo::FleetPersist as _;
use mike::store::persistence::launch::{LaunchSurfaceRefs, NewLaunch, NewRun};
use mike::store::persistence::launch_repo::LaunchPersist as _;
use mike::store::persistence::session_notes::{NewSessionNote, SessionNoteRow};
use mike::store::persistence::test_support::{
    TestDbHandle, acquire_ephemeral_db, seed_fleet_running, seed_project, seed_two_sessions,
};
use serde_json::json;
use uuid::Uuid;

pub struct Fixture {
    pub db: TestDbHandle,
    pub persist: PersistHandle,
    pub row: FleetRow,
    pub run: String,
    pub repo: tempfile::TempDir,
    pub def: FleetDefinition,
    pub reported: String,
    pub checked: String,
    pub old: Uuid,
    pub successor: Uuid,
    pub coordinator: Uuid,
    pub producer: Uuid,
    old_launch: i64,
}

async fn git(repo: &std::path::Path, args: &[&str]) -> anyhow::Result<String> {
    let output = tokio::process::Command::new("git")
        .args(args)
        .current_dir(repo)
        .kill_on_drop(true)
        .output()
        .await?;
    anyhow::ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().into())
}

async fn checkout() -> anyhow::Result<(tempfile::TempDir, String, String)> {
    let repo = tempfile::tempdir()?;
    for directory in ["tests", "src"] {
        std::fs::create_dir(repo.path().join(directory))?;
    }
    for path in ["tests/parts.rs", "tests/ink.test.ts", "src/protocol.rs"] {
        std::fs::write(repo.path().join(path), "fixture")?;
    }
    git(repo.path(), &["init", "-q"]).await?;
    git(repo.path(), &["config", "user.name", "Test"]).await?;
    git(
        repo.path(),
        &["config", "user.email", "test@example.invalid"],
    )
    .await?;
    git(repo.path(), &["config", "commit.gpgsign", "false"]).await?;
    git(repo.path(), &["config", "core.hooksPath", "/dev/null"]).await?;
    git(repo.path(), &["add", "tests", "src"]).await?;
    git(repo.path(), &["commit", "-qm", "reported fixture"]).await?;
    let reported = git(repo.path(), &["rev-parse", "HEAD"]).await?;
    git(
        repo.path(),
        &["commit", "--allow-empty", "-qm", "later gate fixture"],
    )
    .await?;
    let checked = git(repo.path(), &["rev-parse", "HEAD"]).await?;
    Ok((repo, reported, checked))
}

impl Fixture {
    pub async fn new() -> anyhow::Result<Option<Self>> {
        let Some(db) = acquire_ephemeral_db().await? else {
            eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
            return Ok(None);
        };
        let persist = PersistHandle::connect(&db.database_url).await?;
        let (repo, reported, checked) = checkout().await?;
        let project = seed_project(persist.pool(), &repo.path().display().to_string()).await?;
        let [old, successor, coordinator, producer] = std::array::from_fn(|_| Uuid::new_v4());
        for (first, second) in [(old, successor), (coordinator, producer)] {
            seed_two_sessions(
                persist.pool(),
                &first.to_string(),
                &second.to_string(),
                project,
            )
            .await?;
        }
        let run = format!("late-gate-{}", Uuid::new_v4());
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
        let def = FleetDefinition::from_value(&json!({
            "repo": repo.path(), "gate": {"command": "project cross-cutting tests"},
            "coordinator": {"session": coordinator.to_string()},
            "tasks": {
                "A": {"seat": "worker-A", "session": old.to_string(), "files": ["tests/parts.rs"]},
                "B": {"seat": "worker-B", "session": producer.to_string(), "files": ["src/protocol.rs"]}
            }
        })).map_err(anyhow::Error::msg)?;
        let row = persist
            .fleet()
            .insert_fleet(&NewFleet {
                slug: run.clone(),
                project_path: def.repo.clone(),
                definition: serde_json::to_value(&def)?,
                missing: json!([]),
                created_by: None,
            })
            .await?;
        let row = seed_fleet_running(&persist, row.fleet_uuid, &run).await?;
        let mut fixture = Self {
            db,
            persist,
            row,
            run,
            repo,
            def,
            reported,
            checked,
            old,
            successor,
            coordinator,
            producer,
            old_launch: 0,
        };
        fixture.old_launch = fixture.launch(old, "worker-A").await?;
        for (session, seat) in [
            (successor, "worker-A"),
            (coordinator, "coordinator"),
            (producer, "worker-B"),
        ] {
            fixture.launch(session, seat).await?;
        }
        Ok(Some(fixture))
    }

    async fn launch(&self, session: Uuid, seat: &str) -> anyhow::Result<i64> {
        let mut launch = NewLaunch::new("none", &self.def.repo);
        launch.run_id = Some(self.run.clone());
        launch.session_uuid = Some(session);
        launch.seat = Some(seat.into());
        let id = self.persist.launch().insert_launch(&launch).await?;
        self.persist
            .launch()
            .record_launch_opened(id, &LaunchSurfaceRefs::default())
            .await?;
        Ok(id)
    }

    pub async fn report(&self, task: &str) -> anyhow::Result<SessionNoteRow> {
        let session = if task == "A" { self.old } else { self.producer };
        let tags = vec![
            format!("fleet:{}", self.row.fleet_uuid),
            "report".into(),
            format!("task:{task}"),
        ];
        let data = json!({"task": task, "commit": self.reported, "check_command": "narrow check",
                          "check": "passed", "evidence": ["artifacts/scenario.log"]});
        Ok(self
            .persist
            .launch()
            .create_session_note(
                session,
                NewSessionNote {
                    kind: Some("report"),
                    title: "Owned slice",
                    body: Some("Run the recorded scenario and inspect its output"),
                    status: None,
                    tags: Some(&tags),
                    data: Some(&data),
                },
            )
            .await?)
    }

    pub async fn close_owner(&self, replace: bool) -> anyhow::Result<()> {
        if replace {
            let mut current = self.def.clone();
            current.tasks.entry("A".into()).or_default().session = Some(self.successor.to_string());
            anyhow::ensure!(
                self.persist
                    .fleet()
                    .save_fleet_definition(
                        self.row.fleet_uuid,
                        &serde_json::to_value(current)?,
                        &json!([]),
                        self.row.updated_at,
                    )
                    .await?
                    .is_some()
            );
        }
        self.persist
            .launch()
            .record_launch_closed(self.old_launch, "bounded task handed off", Some(true))
            .await?;
        Ok(())
    }

    pub async fn deliver(&self, output: &str) -> anyhow::Result<()> {
        let ledger = mike::launch::fleet::gate::ledger(&self.run);
        let check = self
            .persist
            .fleet()
            .begin_tree_check(&ledger, &self.checked, &self.def.gate.command, "mike")
            .await?;
        self.persist
            .fleet()
            .finish_tree_check(check, Some(101), output, Some(&self.checked))
            .await?;
        gate_failures::deliver(
            &self.persist,
            &self.def,
            &self.run,
            &self.checked,
            (Some(101), output, &self.repo.path().join("unavailable.log")),
        )
        .await;
        Ok(())
    }

    pub async fn inbox(&self, session: Uuid) -> anyhow::Result<String> {
        Ok(self
            .persist
            .launch()
            .fetch_for_session_with_activity(session)
            .await?
            .inbox
            .into_iter()
            .map(|message| message.row.body)
            .collect::<Vec<_>>()
            .join("\n"))
    }

    pub async fn still_closed(&self) -> anyhow::Result<()> {
        let launch = self
            .persist
            .launch()
            .find_launch_by_session(self.old)
            .await?
            .ok_or_else(|| anyhow::anyhow!("old launch disappeared"))?;
        assert!(launch.closed_at.is_some());
        assert!(self.inbox(self.old).await?.is_empty());
        Ok(())
    }
}
