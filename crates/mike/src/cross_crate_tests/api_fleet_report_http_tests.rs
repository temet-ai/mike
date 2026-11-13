use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;

use crate::store::persistence::test_support::seed_fleet_running;
use chrono::Utc;
use reqwest::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::api::fleet::report::{Route, route};
use crate::config::ProxyConfig;
use crate::launch::fleet::FleetDefinition;
use crate::launch::fleet::events::{EventPayload, NewFleetEvent, SeatLaunchConfirmed};
use crate::launch::fleet::seated::Seated;
use crate::launch::fleet::status::{FleetHeader, NoteFacts, build, note_facts};
use crate::launch::fleet::tell_acceptance::read_waiting;
use crate::state::AppState;
use crate::store::persistence::fleet::NewFleet;
use crate::store::persistence::launch::NewRun;
use crate::store::persistence::session_notes::NewSessionNote;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};
use crate::store::persistence::{PersistHandle, SENTINEL_PROJECT_PATH};

const ACCEPTANCE: u128 = 1;
const WORKER: u128 = 14;
const OTHER: u128 = 15;
const RUN: &str = "fleet-report-1";

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

fn seated() -> Value {
    seated_in("/srv/project")
}

fn seated_in(repo: &str) -> Value {
    json!({
        "goal": "reports and verdicts are records",
        "repo": repo,
        "criteria": {"C1": {"text": "stored"}, "C2": {"text": "shown"}},
        "coordinator": {"session": id(2).to_string()},
        "observer": {"session": id(3).to_string()},
        "acceptance": {"session": id(ACCEPTANCE).to_string()},
        "tasks": {
            "P1": {"name": "schema", "discipline": "db-dev", "criteria": ["C1"],
                   "seat": "db-dev-P1", "session": id(WORKER).to_string()},
            "P2": {"name": "page", "discipline": "frontend-developer", "criteria": ["C2"],
                   "seat": "frontend-developer-P2", "session": id(OTHER).to_string()}
        }
    })
}

struct Harness {
    base: String,
    client: reqwest::Client,
    persist: PersistHandle,
    def: FleetDefinition,
    db: TestDbHandle,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
    head: String,
    _repo: tempfile::TempDir,
}

fn git_in(repo: &std::path::Path, args: &[&str]) -> anyhow::Result<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()?;
    anyhow::ensure!(out.status.success(), "git {args:?} failed");
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

impl Harness {
    async fn boot() -> anyhow::Result<Option<Self>> {
        let Some(db) = acquire_ephemeral_db().await? else {
            eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
            return Ok(None);
        };
        let repo = tempfile::tempdir()?;
        git_in(repo.path(), &["init", "-q", "-b", "main"])?;
        git_in(
            repo.path(),
            &["commit", "-q", "--allow-empty", "-m", "work"],
        )?;
        let head = git_in(repo.path(), &["rev-parse", "HEAD"])?;
        let repo_path = repo.path().to_string_lossy().to_string();
        let persist = PersistHandle::connect(&db.database_url).await?;
        for n in [ACCEPTANCE, 2, 3, WORKER, OTHER] {
            persist
                .launch()
                .ensure_session_exists(&id(n).to_string())
                .await?;
        }
        persist
            .launch()
            .upsert_run(&NewRun {
                run_id: RUN.into(),
                project_path: Some(SENTINEL_PROJECT_PATH.into()),
                backend: "none".into(),
                workspace_ref: None,
                tab_ref: None,
            })
            .await?;
        let fleet = persist
            .fleet()
            .insert_fleet(&NewFleet {
                slug: "report".into(),
                project_path: SENTINEL_PROJECT_PATH.into(),
                definition: seated_in(&repo_path),
                missing: json!([]),
                created_by: None,
            })
            .await?;
        seed_fleet_running(&persist, fleet.fleet_uuid, RUN).await?;
        for (task, session) in [
            (Some("P1"), WORKER),
            (Some("P2"), OTHER),
            (None, ACCEPTANCE),
        ] {
            persist
                .fleet()
                .append_fleet_event(&NewFleetEvent {
                    run_id: RUN.into(),
                    task: task.map(str::to_string),
                    session_uuid: Some(id(session)),
                    payload: EventPayload::SeatLaunchConfirmed(SeatLaunchConfirmed {
                        launch_id: 1,
                    }),
                })
                .await?;
        }
        let def =
            FleetDefinition::from_value(&seated_in(&repo_path)).map_err(anyhow::Error::msg)?;
        let mut state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
        state.persist = Some(persist.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let router = crate::app::inspect_router_on(state, listener.local_addr()?);
        let base = format!("http://{}/api/fleets/report", listener.local_addr()?);
        let server = tokio::spawn(async move { axum::serve(listener, router).await });
        Ok(Some(Self {
            base,
            client: reqwest::Client::new(),
            persist,
            def,
            db,
            server,
            head,
            _repo: repo,
        }))
    }

    async fn post(&self, route: &str, body: Value) -> anyhow::Result<(StatusCode, Value)> {
        let response = self
            .client
            .post(format!("{}/{route}", self.base))
            .json(&body)
            .send()
            .await?;
        let status = response.status();
        Ok((status, response.json().await?))
    }

    async fn inbox(&self, session: u128) -> anyhow::Result<Vec<String>> {
        Ok(self
            .persist
            .launch()
            .fetch_for_session_with_activity(id(session))
            .await?
            .inbox
            .into_iter()
            .map(|m| m.row.body)
            .collect())
    }

    async fn committed(&self, sha: &str) -> anyhow::Result<()> {
        let tags = vec!["commit".to_string(), "P1".to_string()];
        let data = json!({"task": "P1", "sha": sha});
        self.persist
            .launch()
            .create_session_note(
                id(WORKER),
                NewSessionNote {
                    kind: Some("result"),
                    title: "P1 committed",
                    body: None,
                    status: None,
                    tags: Some(&tags),
                    data: Some(&data),
                },
            )
            .await?;
        Ok(())
    }

    async fn state_of(&self, task: &str) -> anyhow::Result<String> {
        let rows = self.persist.fleet().fleet_note_rows(RUN).await?;
        let notes: Vec<NoteFacts> = rows.into_iter().map(note_facts).collect();
        let header = FleetHeader {
            fleet: String::new(),
            slug: "report".into(),
            state: "running".into(),
            run_id: Some(RUN.into()),
            started_at: None,
            finished_at: None,
            now: Utc::now(),
            missing: vec![],
        };
        let status = build(header, &self.def, &[], &notes);
        Ok(status
            .tasks
            .into_iter()
            .find(|t| t.id == task)
            .map(|t| t.state)
            .unwrap_or_default())
    }

    async fn cleanup(self) -> anyhow::Result<()> {
        self.server.abort();
        self.db.cleanup().await
    }
}

#[test]
fn a_report_goes_to_the_acceptance_seat_only_while_the_record_seats_it() -> anyhow::Result<()> {
    let def = FleetDefinition::from_value(&seated()).map_err(anyhow::Error::msg)?;
    assert!(
        def.acceptance.session.is_some(),
        "start writes one whether or not a seat exists"
    );
    let nobody = Seated::default();
    assert_eq!(route(&def, &nobody, "0123456789abcdef"), Route::Judge);
    assert_eq!(route(&def, &nobody, ""), Route::NoCommit);
    let acceptance = Seated::default().with_everything(id(ACCEPTANCE), "acceptance");
    assert_eq!(
        route(&def, &acceptance, "0123456789abcdef"),
        Route::Acceptance
    );
    assert_eq!(route(&def, &acceptance, ""), Route::Acceptance);
    Ok(())
}

fn report_body(session: u128, sha: &str) -> Value {
    json!({
        "session": id(session), "task": "P1", "slice": "the table", "criteria": ["C1"],
        "commit": sha,
        "exercised": {"command": "true", "exit": 0, "output_ref": "out.txt"},
        "tests_changed": ["none"], "unestablished": [],
        "look_at": "the route test", "evidence": ["crates/mike/src/store/persistence/improvement/mod.rs"]
    })
}

#[tokio::test]
async fn a_report_and_a_verdict_are_one_call_each_and_each_tells_the_other_seat_once()
-> anyhow::Result<()> {
    let Some(h) = Harness::boot().await? else {
        return Ok(());
    };
    h.committed(&h.head.clone()).await?;
    let (code, refused) = h.post("report", report_body(OTHER, &h.head)).await?;
    assert_eq!(code, StatusCode::BAD_REQUEST, "{refused}");
    let (code, report) = h.post("report", report_body(WORKER, &h.head)).await?;
    assert_eq!(code, StatusCode::OK, "{report}");
    assert_eq!(report["commit"], h.head);
    assert_eq!(report["told_acceptance"], true);
    let note = report["note"].as_str().unwrap_or_default().to_string();
    let mail = h.inbox(ACCEPTANCE).await?;
    assert_eq!(mail.len(), 1, "{mail:?}");
    assert!(
        mail[0].contains(&format!("Report note: {note}.")),
        "{}",
        mail[0]
    );
    assert!(
        mail[0].contains(&format!("Commit: {}.", h.head)),
        "{}",
        mail[0]
    );
    assert_eq!(read_waiting(&h.persist, &h.def, RUN).await.len(), 1);
    assert_eq!(h.state_of("P1").await?, "awaiting acceptance");

    let mut judged = json!({
        "session": id(WORKER), "task": "P1", "criteria": ["C1"], "verdict": "pass",
        "scope": "final", "reason": "the table reads back whole", "report_note": note
    });
    let (code, refused) = h.post("verdict", judged.clone()).await?;
    assert_eq!(code, StatusCode::BAD_REQUEST, "{refused}");
    judged["session"] = json!(id(ACCEPTANCE));
    judged["report_note"] = json!(id(999));
    let (code, refused) = h.post("verdict", judged.clone()).await?;
    assert_eq!(code, StatusCode::BAD_REQUEST, "{refused}");
    judged["report_note"] = json!(note);
    let (code, verdict) = h.post("verdict", judged).await?;
    assert_eq!(code, StatusCode::OK, "{verdict}");
    assert_eq!(verdict["told_seat"], true);
    let told = h.inbox(WORKER).await?;
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(
        told[0].starts_with("[mike verdict] PASS for task P1"),
        "{}",
        told[0]
    );
    assert!(read_waiting(&h.persist, &h.def, RUN).await.is_empty());
    assert_eq!(h.state_of("P1").await?, "passed");
    h.cleanup().await
}

#[path = "api_fleet_ruling_http_tests.rs"]
mod ruling_tests;

#[path = "api_fleet_report_http_tests_more.rs"]
mod more_tests;
