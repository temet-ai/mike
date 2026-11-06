use crate::store::persistence::test_support::seed_fleet_running;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Response;

use serde_json::{Value, json};
use uuid::Uuid;

use super::{PostSessionMessageBody, post_session_message};
use crate::config::{AppState, ProxyConfig};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::change_record::test_seed;
use crate::store::persistence::fleet::NewFleet;
use crate::store::persistence::launch::{LaunchSurfaceRefs, NewLaunch, NewRun};

use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};

const CHECKOUT: &str = "/tmp/mail-fleet";

mod closed_delivery;

struct Env {
    db: TestDbHandle,
    state: AppState,
    persist: PersistHandle,
    project: i64,
    run: String,
}

struct Seat {
    session: Uuid,
    launch: i64,
}

impl Env {
    async fn boot() -> anyhow::Result<Option<Self>> {
        let Some(db) = acquire_ephemeral_db().await? else {
            eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
            return Ok(None);
        };
        let config = ProxyConfig::try_parse_from([
            "mike",
            "--upstream-url",
            "https://api.anthropic.com",
            "--proxy-listen-addr",
            "127.0.0.1:0",
            "--inspect-listen-addr",
            "127.0.0.1:0",
            "--persist-enabled",
            "1",
            "--database-url",
            &db.database_url,
        ])?;
        let state = AppState::new(config).await?;
        let persist = state
            .persist()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("persist_enabled config must yield a PersistHandle"))?;
        let project = test_seed::project(persist.pool()).await?;
        let run = format!("fleet-run-{}", Uuid::new_v4());
        persist
            .upsert_run(&NewRun {
                run_id: run.clone(),
                project_path: Some(CHECKOUT.to_owned()),
                backend: "herdr".to_owned(),
                workspace_ref: None,
                tab_ref: None,
            })
            .await?;
        Ok(Some(Self {
            db,
            state,
            persist,
            project,
            run,
        }))
    }

    async fn cleanup(self) -> anyhow::Result<()> {
        self.state.shutdown().cancel();
        self.db.cleanup().await
    }

    async fn seat(&self, name: &str) -> anyhow::Result<Seat> {
        let id = test_seed::session(self.persist.pool(), self.project, Some(CHECKOUT)).await?;
        let session = self
            .persist
            .session_identities(&[id])
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("no identity for {name}"))?
            .uuid;
        let mut launch = NewLaunch::new("herdr", CHECKOUT);
        launch.run_id = Some(self.run.clone());
        launch.seat = Some(name.to_owned());
        launch.session_uuid = Some(session);
        let launch = self.persist.insert_launch(&launch).await?;
        self.persist
            .record_launch_opened(launch, &LaunchSurfaceRefs::default())
            .await?;
        Ok(Seat { session, launch })
    }

    async fn close(&self, seat: &Seat) -> anyhow::Result<()> {
        self.persist
            .record_launch_closed(seat.launch, "complete", Some(true))
            .await?;
        Ok(())
    }

    async fn define(&self, definition: Value) -> anyhow::Result<()> {
        let fleet = self
            .persist
            .insert_fleet(&NewFleet {
                slug: format!("mail-{}", Uuid::new_v4()),
                project_path: CHECKOUT.to_owned(),
                definition,
                missing: json!([]),
                created_by: None,
            })
            .await?;
        seed_fleet_running(&self.persist, fleet.fleet_uuid, &self.run).await?;
        Ok(())
    }

    async fn send(
        &self,
        from: Option<Uuid>,
        to: &str,
        subject: Option<&str>,
    ) -> (StatusCode, Value) {
        let body = PostSessionMessageBody {
            from_session_id: from.map(|f| f.to_string()),
            message: "please look at the mail path".to_owned(),
            thread_id: None,
            in_reply_to: None,
            subject: subject.map(str::to_owned),
            tags: None,
            context: Vec::new(),
        };
        read(post_session_message(State(self.state.clone()), Path(to.to_owned()), Json(body)).await)
            .await
    }

    async fn inbox_len(&self, session: Uuid) -> anyhow::Result<usize> {
        Ok(self
            .persist
            .fetch_for_session_with_activity(session)
            .await?
            .inbox
            .len())
    }

    fn tail(&self, on: bool) {
        let mut toggles = (**self.state.feature_toggle_overlay.load()).clone();
        toggles.change_record_tail = on;
        self.state.feature_toggle_overlay.store(Arc::new(toggles));
    }
}

async fn read(result: Result<Response, Response>) -> (StatusCode, Value) {
    let response = result.unwrap_or_else(|e| e);
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap_or_default();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn definition(coordinator: Uuid, tasks: &[(&str, &Seat, &str, &str)]) -> Value {
    let tasks: serde_json::Map<String, Value> = tasks
        .iter()
        .map(|(id, seat, name, files)| {
            (
                (*id).to_owned(),
                json!({
                    "session": seat.session.to_string(),
                    "seat": name,
                    "discipline": name.split_once('-').map_or("", |(discipline, _)| discipline),
                    "files": [files],
                }),
            )
        })
        .collect();
    json!({
        "repo": CHECKOUT,
        "coordinator": {"session": coordinator.to_string()},
        "tasks": tasks,
    })
}

struct Fleet {
    env: Env,
    lead: Seat,
    t1: Seat,
    t2: Seat,
}

async fn fleet() -> anyhow::Result<Option<Fleet>> {
    let Some(env) = Env::boot().await? else {
        return Ok(None);
    };
    let lead = env.seat("coordinator").await?;
    let t1 = env.seat("worker-T1").await?;
    let t2 = env.seat("worker-T2").await?;
    env.define(definition(
        lead.session,
        &[
            ("T1", &t1, "worker-T1", "crates/a"),
            ("T2", &t2, "worker-T2", "crates/b"),
        ],
    ))
    .await?;
    Ok(Some(Fleet { env, lead, t1, t2 }))
}

#[tokio::test]
async fn task_form_delivers_to_the_seat_that_has_the_task() -> anyhow::Result<()> {
    let Some(f) = fleet().await? else {
        return Ok(());
    };
    let (status, body) = f.env.send(Some(f.t1.session), "task:T2", None).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["to_session_uuid"], f.t2.session.to_string());
    assert_eq!(f.env.inbox_len(f.t2.session).await?, 1);
    f.env.cleanup().await
}

#[tokio::test]
async fn task_form_names_the_known_tasks_when_the_id_is_wrong() -> anyhow::Result<()> {
    let Some(f) = fleet().await? else {
        return Ok(());
    };
    let (status, body) = f.env.send(Some(f.t1.session), "task:T9", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"], "unknown_task");
    let message = body["message"].as_str().unwrap_or_default();
    assert!(message.contains("T1, T2"), "{message}");
    f.env.cleanup().await
}

#[tokio::test]
async fn owner_form_reaches_the_task_that_owns_the_path_else_the_coordinator() -> anyhow::Result<()>
{
    let Some(f) = fleet().await? else {
        return Ok(());
    };
    let (status, body) = f
        .env
        .send(Some(f.t1.session), "owner:crates/b/src/lib.rs", None)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["to_session_uuid"], f.t2.session.to_string());

    let (status, body) = f
        .env
        .send(Some(f.t1.session), "owner:docs/unowned.md", None)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["to_session_uuid"], f.lead.session.to_string());
    f.env.cleanup().await
}

#[tokio::test]
async fn fleet_form_reaches_every_open_seat_but_not_the_sender_or_a_closed_seat()
-> anyhow::Result<()> {
    let Some(f) = fleet().await? else {
        return Ok(());
    };
    let (status, body) = f
        .env
        .send(Some(f.t1.session), "fleet", Some("heads up"))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(
        body["deliveries"].as_array().map(Vec::len),
        Some(2),
        "{body}"
    );
    assert_eq!(f.env.inbox_len(f.t1.session).await?, 0);
    assert_eq!(f.env.inbox_len(f.t2.session).await?, 1);
    assert_eq!(f.env.inbox_len(f.lead.session).await?, 1);

    f.env.close(&f.t2).await?;
    let (_, body) = f
        .env
        .send(Some(f.t1.session), "fleet", Some("heads up again"))
        .await;
    assert_eq!(
        body["deliveries"].as_array().map(Vec::len),
        Some(1),
        "{body}"
    );
    f.env.cleanup().await
}

#[tokio::test]
async fn fleet_forms_are_refused_for_a_sender_outside_a_fleet() -> anyhow::Result<()> {
    let Some(f) = fleet().await? else {
        return Ok(());
    };
    let outsider_id =
        test_seed::session(f.env.persist.pool(), f.env.project, Some("/tmp/elsewhere")).await?;
    let outsider = f
        .env
        .persist
        .session_identities(&[outsider_id])
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("no identity"))?
        .uuid;
    for to in ["task:T1", "owner:crates/a", "fleet"] {
        let (status, body) = f.env.send(Some(outsider), to, Some("hello")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{to}: {body}");
        assert_eq!(body["error"], "not_in_fleet", "{to}: {body}");
    }
    f.env.cleanup().await
}

#[tokio::test]
async fn a_send_to_a_closed_seat_names_the_seat_that_now_has_its_task() -> anyhow::Result<()> {
    let Some(f) = fleet().await? else {
        return Ok(());
    };
    let replaced = f.env.seat("worker-T2").await?;
    f.env.close(&f.t2).await?;
    f.env
        .define(definition(
            f.lead.session,
            &[
                ("T1", &f.t1, "worker-T1", "crates/a"),
                ("T2", &replaced, "worker-T2", "crates/b"),
            ],
        ))
        .await?;
    let (status, body) = f
        .env
        .send(Some(f.t1.session), &f.t2.session.to_string(), None)
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"], "recipient_closed");
    let message = body["message"].as_str().unwrap_or_default();
    assert!(message.contains(&replaced.session.to_string()), "{message}");
    assert_eq!(f.env.inbox_len(f.t2.session).await?, 0);
    f.env.cleanup().await
}

#[tokio::test]
async fn a_send_to_a_closed_seat_with_no_successor_names_the_coordinator() -> anyhow::Result<()> {
    let Some(f) = fleet().await? else {
        return Ok(());
    };
    f.env.close(&f.t2).await?;
    let (status, body) = f
        .env
        .send(Some(f.t1.session), &f.t2.session.to_string(), None)
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let message = body["message"].as_str().unwrap_or_default();
    assert!(message.contains("coordinator"), "{message}");
    assert!(message.contains(&f.lead.session.to_string()), "{message}");
    f.env.cleanup().await
}

#[tokio::test]
async fn a_send_without_fleet_context_still_refuses_a_closed_fleet_recipient() -> anyhow::Result<()>
{
    let Some(f) = fleet().await? else {
        return Ok(());
    };
    let plain_id =
        test_seed::session(f.env.persist.pool(), f.env.project, Some("/tmp/elsewhere")).await?;
    let plain = f
        .env
        .persist
        .session_identities(&[plain_id])
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("no identity"))?
        .uuid;
    f.env.close(&f.t2).await?;
    let (status, body) = f
        .env
        .send(Some(plain), &f.t2.session.to_string(), None)
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"], "recipient_closed");
    assert_eq!(f.env.inbox_len(f.t2.session).await?, 0);
    f.env.cleanup().await
}

mod envelope;

#[tokio::test]
async fn send_without_sender_refuses_closed_seat_and_task_form_falls_back_to_coordinator()
-> anyhow::Result<()> {
    let Some(f) = fleet().await? else {
        return Ok(());
    };
    f.env.close(&f.t2).await?;
    let (status, body) = f.env.send(None, &f.t2.session.to_string(), None).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"], "recipient_closed");
    for address in ["task:T2", "owner:crates/b/src/lib.rs"] {
        let (status, body) = f.env.send(Some(f.t1.session), address, None).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        assert_eq!(body["to_session_uuid"], f.lead.session.to_string());
    }
    assert_eq!(f.env.inbox_len(f.t2.session).await?, 0);
    f.env.cleanup().await
}
