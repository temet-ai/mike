//! Moved from crates/mike/src/api/project_knowledge/registration_tests.rs by the crate split: it reaches the serve
//! crate, so it is tested from there.

use crate::config::ProxyConfig;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};
use reqwest::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

struct Harness {
    base: String,
    client: reqwest::Client,
    handle: PersistHandle,
    db: TestDbHandle,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
}

impl Harness {
    async fn boot() -> anyhow::Result<Self> {
        let db = acquire_ephemeral_db()
            .await?
            .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
        let handle = PersistHandle::connect(&db.database_url).await?;
        let mut state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
        state.persist = Some(handle.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let router = crate::app::inspect_router_on(state, listener.local_addr()?);
        let base = format!("http://{}", listener.local_addr()?);
        let server = tokio::spawn(async move { axum::serve(listener, router).await });
        Ok(Self {
            base,
            client: reqwest::Client::new(),
            handle,
            db,
            server,
        })
    }

    async fn post(&self, route: &str, body: Value) -> anyhow::Result<(StatusCode, Value)> {
        let response = self
            .client
            .post(format!("{}{route}", self.base))
            .json(&body)
            .send()
            .await?;
        let status = response.status();
        Ok((status, response.json().await?))
    }

    async fn project(&self, name: &str) -> anyhow::Result<String> {
        let (status, body) = self
            .post("/api/knowledge/projects", json!({"name": name}))
            .await?;
        anyhow::ensure!(status.is_success(), "define project: {status}: {body}");
        Ok(body["project_uuid"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("project_uuid missing: {body}"))?
            .to_string())
    }

    async fn location(
        &self,
        project: &str,
        machine: Uuid,
        root: &str,
    ) -> anyhow::Result<(StatusCode, Value)> {
        self.post(
            "/api/knowledge/locations",
            json!({"project": project, "machine_uuid": machine,
                   "machine_name": "machine", "root_path": root}),
        )
        .await
    }

    async fn location_uuid(
        &self,
        project: &str,
        machine: Uuid,
        root: &str,
    ) -> anyhow::Result<Uuid> {
        let (status, body) = self.location(project, machine, root).await?;
        anyhow::ensure!(status.is_success(), "register location: {status}: {body}");
        Ok(serde_json::from_value(body["location_uuid"].clone())?)
    }

    async fn register(&self, session: Uuid, location: Uuid) -> anyhow::Result<(StatusCode, Value)> {
        self.post(
            "/api/workspace/register",
            json!({"session_id": session, "cwd": "/somewhere/else",
                   "location_uuid": location, "registered_via": "launcher"}),
        )
        .await
    }

    async fn bound_location(&self, session: Uuid) -> anyhow::Result<Option<Uuid>> {
        Ok(sqlx::query_scalar(
            "SELECT location_uuid FROM sessions.sessions WHERE session_uuid = $1",
        )
        .bind(session)
        .fetch_one(self.handle.pool())
        .await?)
    }

    async fn location_rows(&self) -> anyhow::Result<i64> {
        Ok(
            sqlx::query_scalar("SELECT count(*) FROM projects.locations")
                .fetch_one(self.handle.pool())
                .await?,
        )
    }

    async fn cleanup(self) -> anyhow::Result<()> {
        self.server.abort();
        self.db.cleanup().await
    }
}

#[tokio::test]
async fn location_registration_of_a_root_owned_by_another_project_is_409() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let owner = harness.project("Owner").await?;
    let intruder = harness.project("Intruder").await?;
    let machine = Uuid::new_v4();
    let owned = harness
        .location_uuid(&owner, machine, "/work/shared")
        .await?;

    let (status, body) = harness.location(&intruder, machine, "/work/shared").await?;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|e| e.contains("another project"))
    );
    let again = harness
        .location_uuid(&owner, machine, "/work/shared")
        .await?;
    assert_eq!(again, owned);
    assert_eq!(harness.location_rows().await?, 1);
    harness.cleanup().await
}

#[tokio::test]
async fn location_registration_repeated_returns_the_same_location_without_a_duplicate_row()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let project = harness.project("Repeated").await?;
    let machine = Uuid::new_v4();

    let first = harness
        .location_uuid(&project, machine, "/work/app")
        .await?;
    let second = harness
        .location_uuid(&project, machine, "/work/app")
        .await?;

    assert_eq!(first, second);
    assert_eq!(harness.location_rows().await?, 1);
    harness.cleanup().await
}

#[tokio::test]
async fn workspace_register_with_a_location_resolves_the_project_as_operator() -> anyhow::Result<()>
{
    let harness = Harness::boot().await?;
    let project = harness.project("Portable").await?;
    let location = harness
        .location_uuid(&project, Uuid::new_v4(), "/Users/dev/app")
        .await?;
    let session = Uuid::new_v4();

    let (status, body) = harness.register(session, location).await?;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["project_resolution"], "operator");
    assert_eq!(harness.bound_location(session).await?, Some(location));
    harness.cleanup().await
}

#[tokio::test]
async fn workspace_register_repeated_with_the_same_location_is_idempotent() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let project = harness.project("Reconnect").await?;
    let machine = Uuid::new_v4();
    let location = harness
        .location_uuid(&project, machine, "/work/app")
        .await?;
    let session = Uuid::new_v4();

    let (first_status, first) = harness.register(session, location).await?;
    let relocated = harness
        .location_uuid(&project, machine, "/work/app")
        .await?;
    let (second_status, second) = harness.register(session, relocated).await?;

    assert_eq!(first_status, StatusCode::OK, "{first}");
    assert_eq!(second_status, StatusCode::OK, "{second}");
    assert_eq!(first["project_id"], second["project_id"]);
    assert_eq!(second["project_resolution"], "operator");
    assert_eq!(harness.bound_location(session).await?, Some(location));
    assert_eq!(harness.location_rows().await?, 1);
    harness.cleanup().await
}

#[tokio::test]
async fn workspace_register_with_a_location_of_another_project_is_409_and_keeps_the_binding()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let first = harness.project("First").await?;
    let second = harness.project("Second").await?;
    let machine = Uuid::new_v4();
    let owned = harness
        .location_uuid(&first, machine, "/work/first")
        .await?;
    let foreign = harness
        .location_uuid(&second, machine, "/work/second")
        .await?;
    let session = Uuid::new_v4();
    let (status, body) = harness.register(session, owned).await?;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, body) = harness.register(session, foreign).await?;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(harness.bound_location(session).await?, Some(owned));
    harness.cleanup().await
}

#[tokio::test]
async fn workspace_register_with_an_unknown_location_is_409() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let session = Uuid::new_v4();

    let (status, body) = harness.register(session, Uuid::new_v4()).await?;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    harness.cleanup().await
}
