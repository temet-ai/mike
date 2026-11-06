use mike::config::{AppState, ProxyConfig};
use mike::launch::fleet::status::{FleetStatus, assemble};
use mike::store::persistence::PersistHandle;
use mike::store::persistence::fleet::{FleetRow, NewFleet};
use mike::store::persistence::launch::{LaunchSurfaceRefs, NewLaunch, NewRun};
use mike::store::persistence::session_notes::{NewSessionNote, SessionNoteRow, SessionNoteUpdate};
use mike::store::persistence::test_support::{
    TestDbHandle, acquire_ephemeral_db, seed_fleet_starting,
};
use serde_json::{Value, json};
use uuid::Uuid;

pub(super) struct Fixture {
    pub(super) persist: PersistHandle,
    pub(super) row: FleetRow,
    pub(super) acceptance: Uuid,
    pub(super) worker: Uuid,
    pub(super) run: String,
    state: AppState,
    db: TestDbHandle,
}

impl Fixture {
    pub(super) async fn new() -> anyhow::Result<Option<Self>> {
        let Some(db) = acquire_ephemeral_db().await? else {
            eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
            return Ok(None);
        };
        let persist = PersistHandle::connect(&db.database_url).await?;
        let acceptance = Uuid::new_v4();
        let worker = Uuid::new_v4();
        for session in [acceptance, worker] {
            persist.ensure_session_exists(&session.to_string()).await?;
        }
        let run = format!("obligation-refresh-{}", Uuid::new_v4());
        let path = format!("/tmp/{run}");
        persist
            .upsert_run(&NewRun {
                run_id: run.clone(),
                project_path: Some(path.clone()),
                backend: "none".into(),
                workspace_ref: None,
                tab_ref: None,
            })
            .await?;
        let row = persist
            .insert_fleet(&NewFleet {
                slug: run.clone(),
                project_path: path.clone(),
                definition: json!({
                    "repo": path,
                    "acceptance": {"session": acceptance},
                    "criteria": {"C1": {"text": "setting persists"}, "C2": {"text": "other task"}},
                    "tasks": {
                        "T1": {"name": "setting", "discipline": "rust", "criteria": ["C1"],
                               "session": worker, "seat": "rust-T1"},
                        "T2": {"name": "other", "discipline": "rust", "criteria": ["C2"]}
                    }
                }),
                missing: json!([]),
                created_by: None,
            })
            .await?;
        let row = seed_fleet_starting(&persist, row.fleet_uuid, Some(&run)).await?;
        for (session, seat, role) in [
            (acceptance, "acceptance", "acceptance"),
            (worker, "rust-T1", "rust"),
        ] {
            launch(&persist, &run, &path, (session, seat, role)).await?;
        }
        let row = persist
            .transition_fleet_state(&row, "running", None, None)
            .await?
            .ok_or_else(|| anyhow::anyhow!("fixture did not start"))?;
        let mut state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
        state.persist = Some(persist.clone());
        Ok(Some(Self {
            persist,
            row,
            acceptance,
            worker,
            run,
            state,
            db,
        }))
    }

    pub(super) fn tags(&self) -> Vec<String> {
        vec![
            "acceptance-obligation".into(),
            format!("fleet:{}", self.row.fleet_uuid),
        ]
    }

    pub(super) async fn obligation(&self, data: &Value) -> anyhow::Result<SessionNoteRow> {
        Ok(self
            .persist
            .create_session_note(
                self.acceptance,
                NewSessionNote {
                    kind: Some("task"),
                    title: "Observe the persisted setting",
                    body: Some("Full scenario, unchanged observations and ownership"),
                    status: Some("open"),
                    tags: Some(&self.tags()),
                    data: Some(data),
                },
            )
            .await?)
    }

    pub(super) async fn verdict(&self) -> anyhow::Result<SessionNoteRow> {
        let data = json!({
            "tasks": ["T1", "T2"], "criteria": ["C1", "C2"], "verdict": "pass", "scope": "final"
        });
        let tags = [
            format!("fleet:{}", self.row.fleet_uuid),
            "fleet-verdict".into(),
        ];
        Ok(self
            .persist
            .create_session_note(
                self.acceptance,
                NewSessionNote {
                    kind: Some("decision"),
                    title: "Final pass",
                    body: Some(
                        "The final verdict must not be re-recorded for an observation update",
                    ),
                    status: None,
                    tags: Some(&tags),
                    data: Some(&data),
                },
            )
            .await?)
    }

    pub(super) async fn outcome(
        &self,
        note: &SessionNoteRow,
        outcome: &str,
    ) -> anyhow::Result<SessionNoteRow> {
        let mut data = note.data.clone();
        data["outcome"] = json!(outcome);
        self.persist
            .update_session_note(
                note.note_uuid,
                SessionNoteUpdate {
                    data: Some(&data),
                    ..Default::default()
                },
            )
            .await?
            .ok_or_else(|| anyhow::anyhow!("obligation disappeared"))
    }

    pub(super) async fn status(&self) -> anyhow::Result<FleetStatus> {
        assemble(&self.state, &self.row)
            .await
            .map_err(anyhow::Error::msg)
    }

    pub(super) async fn unchanged_verdict(&self, verdict: &SessionNoteRow) -> anyhow::Result<()> {
        let notes = self.persist.fleet_note_rows(&self.run).await?;
        let decisions: Vec<_> = notes
            .iter()
            .filter(|note| note.kind == "decision")
            .collect();
        assert_eq!(decisions.len(), 1);
        let found = decisions[0];
        assert_eq!(found.note_uuid, verdict.note_uuid);
        assert_eq!(found.updated_at, verdict.updated_at);
        assert_eq!(found.data, verdict.data);
        assert_eq!(found.body, verdict.body);
        assert_eq!(found.session_uuid, self.acceptance);
        Ok(())
    }

    pub(super) async fn cleanup(self) -> anyhow::Result<()> {
        self.db.cleanup().await
    }
}

async fn launch(
    persist: &PersistHandle,
    run: &str,
    path: &str,
    (session, seat, role): (Uuid, &str, &str),
) -> anyhow::Result<()> {
    let mut launch = NewLaunch::new("none", path);
    launch.run_id = Some(run.into());
    launch.session_uuid = Some(session);
    launch.seat = Some(seat.into());
    launch.identity = Some(json!({"role": role}));
    let id = persist.insert_launch(&launch).await?;
    persist
        .record_launch_opened(id, &LaunchSurfaceRefs::default())
        .await?;
    Ok(())
}

pub(super) fn obligation(outcome: &str) -> Value {
    json!({
        "obligation_id": "setting-journey", "task": "T1", "criteria": ["C1"],
        "outcome": outcome, "observation": "Changed the setting, reopened it and observed the persisted value"
    })
}

pub(super) fn assert_status(status: &FleetStatus, first: &str, accepted: bool) {
    assert_eq!(status.tasks[0].id, "T1");
    assert_eq!(status.tasks[0].state, first);
    assert_eq!(status.tasks[1].id, "T2");
    assert_eq!(status.tasks[1].state, "passed");
    assert_eq!(status.accepted, accepted);
    assert_eq!(status.criteria[0].passed, usize::from(first == "passed"));
    assert_eq!(status.criteria[1].passed, 1);
}
