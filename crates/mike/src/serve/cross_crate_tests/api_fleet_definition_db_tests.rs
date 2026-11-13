//! Moved from crates/mike/src/api/fleet/definition_db_tests.rs by the crate split: it reaches the serve
//! crate, so it is tested from there.
use crate::store::persistence::fleet_repo::FleetPersist as _;

use crate::store::persistence::test_support::seed_fleet_starting;
use reqwest::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::api::fleet::definition_patch::{PatchFleetRequest, save_definition};
use crate::config::ProxyConfig;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};

struct Harness {
    base: String,
    client: reqwest::Client,
    state: AppState,
    handle: PersistHandle,
    db: TestDbHandle,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
}

impl Harness {
    async fn boot() -> anyhow::Result<Option<Self>> {
        let Some(db) = acquire_ephemeral_db().await? else {
            eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
            return Ok(None);
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let mut state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
        state.persist = Some(handle.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let router = crate::app::inspect_router_on(state.clone(), listener.local_addr()?);
        let base = format!("http://{}", listener.local_addr()?);
        let server = tokio::spawn(async move { axum::serve(listener, router).await });
        Ok(Some(Self {
            base,
            client: reqwest::Client::new(),
            state,
            handle,
            db,
            server,
        }))
    }

    async fn send(&self, request: reqwest::RequestBuilder) -> anyhow::Result<(StatusCode, Value)> {
        let response = request.send().await?;
        let status = response.status();
        Ok((status, response.json().await?))
    }

    async fn get(&self, route: &str) -> anyhow::Result<(StatusCode, Value)> {
        self.send(self.client.get(format!("{}{route}", self.base)))
            .await
    }

    async fn post(&self, route: &str, body: Value) -> anyhow::Result<(StatusCode, Value)> {
        self.send(
            self.client
                .post(format!("{}{route}", self.base))
                .json(&body),
        )
        .await
    }

    async fn patch(&self, fleet: &str, body: Value) -> anyhow::Result<(StatusCode, Value)> {
        let url = format!("{}/api/fleets/{fleet}", self.base);
        self.send(self.client.patch(url).json(&body)).await
    }

    async fn create(&self, slug: &str) -> anyhow::Result<Value> {
        let (status, body) = self
            .post(
                "/api/fleets",
                json!({"slug": slug, "project_path": "/repo"}),
            )
            .await?;
        anyhow::ensure!(status == StatusCode::CREATED, "create: {status}: {body}");
        Ok(body)
    }

    async fn cleanup(self) -> anyhow::Result<()> {
        self.server.abort();
        self.db.cleanup().await
    }
}

fn fleet_id(view: &Value) -> anyhow::Result<String> {
    Ok(view["fleet"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("fleet id missing: {view}"))?
        .to_string())
}

fn missing_items(view: &Value) -> Vec<String> {
    view["missing"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|m| m["item"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn missing_problems(view: &Value) -> Vec<String> {
    view["missing"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|m| m["problem"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn create_answers_201_with_a_draft_view_that_lists_what_is_missing() -> anyhow::Result<()> {
    let Some(h) = Harness::boot().await? else {
        return Ok(());
    };
    let view = h.create("night-run").await?;
    assert_eq!(view["slug"], "night-run");
    assert_eq!(view["project_path"], "/repo");
    assert_eq!(view["state"], "draft");
    assert_eq!(view["ready"], false);
    assert_eq!(view["definition"], json!({"briefs": {"as_notes": true}}));
    assert!(missing_items(&view).contains(&"goal".to_string()), "{view}");
    h.cleanup().await
}

#[tokio::test]
async fn create_with_a_definition_stores_it_and_checks_it() -> anyhow::Result<()> {
    let Some(h) = Harness::boot().await? else {
        return Ok(());
    };
    let (status, view) = h
        .post(
            "/api/fleets",
            json!({"slug": "seeded", "project_path": "/repo", "definition": {"goal": "ship it"}}),
        )
        .await?;
    assert_eq!(status, StatusCode::CREATED, "{view}");
    assert_eq!(view["definition"]["goal"], "ship it");
    assert!(
        !missing_items(&view).contains(&"goal".to_string()),
        "{view}"
    );
    h.cleanup().await
}

#[tokio::test]
async fn create_refuses_a_bad_slug_a_bad_path_and_a_bad_definition() -> anyhow::Result<()> {
    let Some(h) = Harness::boot().await? else {
        return Ok(());
    };
    let looks_like_an_id = Uuid::new_v4().to_string();
    for slug in ["two words", "", looks_like_an_id.as_str()] {
        let (status, body) = h
            .post(
                "/api/fleets",
                json!({"slug": slug, "project_path": "/repo"}),
            )
            .await?;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{slug}: {body}");
        assert_eq!(body["error"], "invalid_request");
    }
    let (status, body) = h
        .post("/api/fleets", json!({"slug": "ok", "project_path": "  "}))
        .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (status, body) = h
        .post(
            "/api/fleets",
            json!({"slug": "ok", "project_path": "/repo", "definition": {"tasks": "many"}}),
        )
        .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"], "invalid_definition");
    let (_, listed) = h.get("/api/fleets").await?;
    assert_eq!(listed["fleets"], json!([]), "nothing may be stored");
    h.cleanup().await
}

#[tokio::test]
async fn get_finds_a_fleet_by_uuid_and_by_slug_and_404s_on_neither() -> anyhow::Result<()> {
    let Some(h) = Harness::boot().await? else {
        return Ok(());
    };
    let created = h.create("by-slug").await?;
    let id = fleet_id(&created)?;
    let (status, by_id) = h.get(&format!("/api/fleets/{id}")).await?;
    assert_eq!(status, StatusCode::OK);
    let (status, by_slug) = h.get("/api/fleets/by-slug").await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(by_id["fleet"], by_slug["fleet"]);
    let (status, body) = h.get("/api/fleets/no-such-fleet").await?;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"], "fleet_not_found");
    let (status, body) = h.get(&format!("/api/fleets/{}", Uuid::new_v4())).await?;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    h.cleanup().await
}

#[tokio::test]
async fn list_omits_definitions_filters_by_project_and_honours_the_limit() -> anyhow::Result<()> {
    let Some(h) = Harness::boot().await? else {
        return Ok(());
    };
    h.create("first").await?;
    h.create("second").await?;
    let (_, other) = h
        .post(
            "/api/fleets",
            json!({"slug": "elsewhere", "project_path": "/other"}),
        )
        .await?;
    assert_eq!(other["state"], "draft");
    let (status, all) = h.get("/api/fleets").await?;
    assert_eq!(status, StatusCode::OK);
    let fleets = all["fleets"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("{all}"))?;
    assert_eq!(fleets.len(), 3);
    assert!(fleets.iter().all(|f| f.get("definition").is_none()));
    let (_, scoped) = h.get("/api/fleets?project_path=/repo").await?;
    assert_eq!(scoped["fleets"].as_array().map(Vec::len), Some(2));
    let (_, limited) = h.get("/api/fleets?limit=1").await?;
    assert_eq!(limited["fleets"].as_array().map(Vec::len), Some(1));
    h.cleanup().await
}

#[tokio::test]
async fn patch_merges_into_the_stored_definition_and_null_removes_an_item() -> anyhow::Result<()> {
    let Some(h) = Harness::boot().await? else {
        return Ok(());
    };
    let id = fleet_id(&h.create("merge").await?)?;
    let (status, view) = h
        .patch(
            &id,
            json!({"patch": {"goal": "ship it", "criteria": {"c1": {"text": "works", "check": "run it"}}}}),
        )
        .await?;
    assert_eq!(status, StatusCode::OK, "{view}");
    assert_eq!(view["definition"]["goal"], "ship it");
    assert!(
        !missing_items(&view).contains(&"goal".to_string()),
        "{view}"
    );
    let (_, view) = h
        .patch(
            &id,
            json!({"patch": {"ask": "the words", "criteria": {"c2": {"text": "b", "check": "c"}}}}),
        )
        .await?;
    assert_eq!(view["definition"]["goal"], "ship it");
    assert_eq!(view["definition"]["criteria"]["c1"]["text"], "works");
    assert_eq!(view["definition"]["criteria"]["c2"]["text"], "b");
    let (_, view) = h.patch(&id, json!({"patch": {"goal": null}})).await?;
    assert!(view["definition"].get("goal").is_none(), "{view}");
    assert!(missing_items(&view).contains(&"goal".to_string()), "{view}");
    assert_eq!(view["definition"]["ask"], "the words");
    h.cleanup().await
}

#[tokio::test]
async fn patch_by_slug_works_and_an_unknown_fleet_is_404() -> anyhow::Result<()> {
    let Some(h) = Harness::boot().await? else {
        return Ok(());
    };
    h.create("slugged").await?;
    let (status, view) = h.patch("slugged", json!({"patch": {"goal": "g"}})).await?;
    assert_eq!(status, StatusCode::OK, "{view}");
    let (status, body) = h
        .patch("missing-fleet", json!({"patch": {"goal": "g"}}))
        .await?;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"], "fleet_not_found");
    h.cleanup().await
}

#[tokio::test]
async fn patch_with_the_wrong_shape_is_400_names_the_item_and_saves_nothing() -> anyhow::Result<()>
{
    let Some(h) = Harness::boot().await? else {
        return Ok(());
    };
    let id = fleet_id(&h.create("shape").await?)?;
    h.patch(&id, json!({"patch": {"goal": "keep me"}})).await?;
    let (status, body) = h
        .patch(
            &id,
            json!({"patch": {"goal": "lost", "workers": {"rust": {"contract": 2}}}}),
        )
        .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"], "invalid_definition");
    let message = body["message"].as_str().unwrap_or_default();
    assert!(message.contains("`workers.rust.contract`"), "{message}");
    let (_, stored) = h.get(&format!("/api/fleets/{id}")).await?;
    assert_eq!(
        stored["definition"],
        json!({"briefs": {"as_notes": true}, "goal": "keep me"})
    );
    let (status, body) = h.patch(&id, json!({"patch": "goal"})).await?;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    h.cleanup().await
}

#[tokio::test]
async fn patch_while_the_fleet_is_starting_is_409_fleet_starting() -> anyhow::Result<()> {
    let Some(h) = Harness::boot().await? else {
        return Ok(());
    };
    let id = fleet_id(&h.create("busy").await?)?;
    seed_fleet_starting(&h.handle, Uuid::parse_str(&id)?, None).await?;
    let (status, body) = h.patch(&id, json!({"patch": {"goal": "g"}})).await?;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"], "fleet_starting");
    let (_, stored) = h.get(&format!("/api/fleets/{id}")).await?;
    assert_eq!(stored["definition"], json!({"briefs": {"as_notes": true}}));
    h.cleanup().await
}

#[tokio::test]
async fn patch_ready_checks_the_machine_and_a_fleet_with_items_missing_stays_draft()
-> anyhow::Result<()> {
    let Some(h) = Harness::boot().await? else {
        return Ok(());
    };
    let id = fleet_id(&h.create("not-yet").await?)?;
    let patch = json!({"goal": "g", "repo": "/no/such/folder", "workers": {"ts": {"agent": "ghost-agent"}}});
    let (status, plain) = h.patch(&id, json!({"patch": patch})).await?;
    assert_eq!(status, StatusCode::OK, "{plain}");
    let problems = missing_problems(&plain);
    assert!(
        !problems.iter().any(|p| p.contains("is not a folder")),
        "{problems:?}"
    );
    let (status, view) = h.patch(&id, json!({"ready": true})).await?;
    assert_eq!(status, StatusCode::OK, "{view}");
    assert_eq!(view["state"], "draft");
    assert_eq!(view["ready"], false);
    let problems = missing_problems(&view);
    assert!(
        problems.iter().any(|p| p.contains("is not a folder")),
        "{problems:?}"
    );
    assert!(
        problems
            .iter()
            .any(|p| p.contains("no agent file for `ghost-agent`")),
        "{problems:?}"
    );
    assert!(
        missing_items(&view).contains(&"criteria".to_string()),
        "{view}"
    );
    assert_eq!(view["ready_at"], Value::Null);
    h.cleanup().await
}

#[tokio::test]
async fn a_ready_fleet_whose_definition_breaks_goes_back_to_draft() -> anyhow::Result<()> {
    let Some(h) = Harness::boot().await? else {
        return Ok(());
    };
    let id = fleet_id(&h.create("regress").await?)?;
    h.handle
        .fleet()
        .set_fleet_state(Uuid::parse_str(&id)?, "ready", None, None)
        .await?;
    let (status, view) = h.patch(&id, json!({"patch": {"goal": "g"}})).await?;
    assert_eq!(status, StatusCode::OK, "{view}");
    assert_eq!(view["state"], "draft", "{view}");
    h.cleanup().await
}

#[tokio::test]
async fn a_stale_read_is_saved_on_the_retry_and_keeps_the_other_writers_change()
-> anyhow::Result<()> {
    let Some(h) = Harness::boot().await? else {
        return Ok(());
    };
    let id = fleet_id(&h.create("race").await?)?;
    let stale = h
        .handle
        .fleet()
        .find_fleet(&id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("fleet vanished"))?;
    h.patch(&id, json!({"patch": {"goal": "from the other writer"}}))
        .await?;
    let body = PatchFleetRequest {
        patch: Some(json!({"ask": "from this writer"})),
        ready: false,
    };
    let saved = save_definition(&h.state, &h.handle, stale, &body).await?;
    assert_eq!(saved.row.definition["goal"], "from the other writer");
    assert_eq!(saved.row.definition["ask"], "from this writer");
    h.cleanup().await
}

#[tokio::test]
async fn start_refuses_a_fleet_that_is_not_ready() -> anyhow::Result<()> {
    let Some(h) = Harness::boot().await? else {
        return Ok(());
    };
    let id = fleet_id(&h.create("early").await?)?;
    let (status, body) = h
        .post(&format!("/api/fleets/{id}/start"), json!({}))
        .await?;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"], "fleet_not_ready");
    let (status, body) = h.post("/api/fleets/nobody/start", json!({})).await?;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    h.cleanup().await
}

#[tokio::test]
async fn status_of_a_fleet_that_has_not_started_lists_what_is_missing() -> anyhow::Result<()> {
    let Some(h) = Harness::boot().await? else {
        return Ok(());
    };
    let id = fleet_id(&h.create("quiet").await?)?;
    h.patch(&id, json!({"patch": {"goal": "the goal"}})).await?;
    let (status, body) = h.get(&format!("/api/fleets/{id}/status")).await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["fleet"], id);
    assert_eq!(body["state"], "draft");
    assert_eq!(body["goal"], "the goal");
    assert!(body["missing"].as_array().is_some_and(|m| !m.is_empty()));
    let (status, _) = h.get("/api/fleets/nobody/status").await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["accepted"], false);
    assert_eq!(body["obligations"], json!([]));
    let (status, body) = h
        .get(&format!("/api/fleets/by-session/{}/status", Uuid::new_v4()))
        .await?;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    h.cleanup().await
}

#[tokio::test]
async fn new_fleet_imports_project_operations_without_starting_seats() -> anyhow::Result<()> {
    let Some(harness) = Harness::boot().await? else {
        return Ok(());
    };
    let project = tempfile::tempdir()?;
    std::fs::create_dir_all(project.path().join(".claude/fleet"))?;
    std::fs::write(
        project.path().join(".claude/fleet/operations.json"),
        r#"{"schema_version": 1, "execution": {"strip_environment": ["BUILD_DIR"], "operations": {"frontend": {
            "command": "scripts/sb-test",
            "purpose": "Frontend type, logic and browser checks",
            "resources": ["mike-frontend-tests"]}}}}"#,
    )?;
    let (status, view) = harness
        .post(
            "/api/fleets",
            json!({
                "slug": format!("profile-{}", Uuid::new_v4()), "project_path": project.path()
            }),
        )
        .await?;
    assert_eq!(status, StatusCode::CREATED, "{view}");
    assert_eq!(view["state"], "draft");
    assert_eq!(
        view["definition"]["execution"]["strip_environment"],
        json!(["BUILD_DIR"])
    );
    assert_eq!(
        view["definition"]["execution"]["operations"]["frontend"]["command"],
        "scripts/sb-test"
    );
    harness.cleanup().await
}
