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

pub async fn stub_jev(status: u16, body: Value) -> wiremock::MockServer {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/alpha/decisions"))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&server)
        .await;
    server
}

pub fn jev_decider(
    server: &wiremock::MockServer,
    persist: &PersistHandle,
    point: &'static str,
    mode: crate::llm::decisions::record::DecisionMode,
) -> anyhow::Result<crate::llm::decisions::point::Decider> {
    let client = crate::llm::OpenRouterClient::new(
        "sk-test".to_owned(),
        "typesafe/jev-1.14".to_owned(),
        Some(server.uri()),
        None,
    )?;
    crate::llm::decisions::point::Decider::with_mode(
        &client,
        persist,
        &crate::settings::SettingsOverlay::default(),
        point,
        mode,
    )
    .ok_or_else(|| anyhow::anyhow!("no decider"))
}

pub fn choice_body(question: &str, key: &str, others: &[&str]) -> Value {
    let mut probabilities = serde_json::Map::new();
    for other in others {
        probabilities.insert((*other).to_owned(), json!(0.1));
    }
    probabilities.insert(key.to_owned(), json!(0.8));
    json!({
        "model": "typesafe/jev-1.14",
        "answers": {question: {"type": "choice", "choice": key, "probabilities": probabilities}},
        "usage": {"input_tokens": 10, "output_tokens": 1, "cost": 0.00001}
    })
}

pub fn noul_body(answers: &[(&str, f64)]) -> Value {
    let mut map = serde_json::Map::new();
    for (name, p) in answers {
        map.insert((*name).to_owned(), json!({"type": "noul", "noul": p}));
    }
    json!({
        "model": "typesafe/jev-1.14",
        "answers": map,
        "usage": {"input_tokens": 10, "output_tokens": 1, "cost": 0.00001}
    })
}
