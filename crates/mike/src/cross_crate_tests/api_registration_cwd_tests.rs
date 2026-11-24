use crate::config::ProxyConfig;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};
use serde_json::{Value, json};
use uuid::Uuid;

struct Harness {
    base: String,
    client: reqwest::Client,
    handle: PersistHandle,
    machine: Uuid,
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
        let machine = match state.host_machine_uuid.get() {
            Some(resolved) => *resolved,
            None => {
                let generated = Uuid::new_v4();
                state
                    .host_machine_uuid
                    .set(generated)
                    .map_err(|_| anyhow::anyhow!("host machine identity resolved concurrently"))?;
                generated
            }
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let router = crate::app::inspect_router_on(state, listener.local_addr()?);
        let base = format!("http://{}", listener.local_addr()?);
        let server = tokio::spawn(async move { axum::serve(listener, router).await });
        Ok(Self {
            base,
            client: reqwest::Client::new(),
            handle,
            machine,
            db,
            server,
        })
    }

    async fn post(&self, route: &str, body: Value) -> anyhow::Result<Value> {
        let response = self
            .client
            .post(format!("{}{route}", self.base))
            .json(&body)
            .send()
            .await?;
        let status = response.status();
        let body: Value = response.json().await?;
        anyhow::ensure!(status.is_success(), "{route}: {status}: {body}");
        Ok(body)
    }

    async fn location(&self, name: &str, root: &str) -> anyhow::Result<Uuid> {
        let project = self
            .post("/api/knowledge/projects", json!({"name": name}))
            .await?;
        let location = self
            .post(
                "/api/knowledge/locations",
                json!({"project": project["project_uuid"], "machine_uuid": self.machine,
                       "machine_name": "host", "root_path": root}),
            )
            .await?;
        Ok(serde_json::from_value(location["location_uuid"].clone())?)
    }

    async fn hook_register(&self, session: Uuid, cwd: &str) -> anyhow::Result<Value> {
        self.post(
            "/api/workspace/register",
            json!({"session_id": session, "cwd": cwd, "registered_via": "hook"}),
        )
        .await
    }

    async fn binding(&self, session: Uuid) -> anyhow::Result<(Option<Uuid>, String)> {
        Ok(sqlx::query_as(
            "SELECT location_uuid, project_resolution FROM sessions.sessions WHERE session_uuid = $1",
        )
        .bind(session)
        .fetch_one(self.handle.pool())
        .await?)
    }

    async fn cleanup(self) -> anyhow::Result<()> {
        self.server.abort();
        self.db.cleanup().await
    }
}

#[tokio::test]
async fn hook_register_cwd_inside_known_location_binds_it() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let location = harness.location("Repo", "/nonexistent/checkouts/repo").await?;
    let session = Uuid::new_v4();

    let body = harness
        .hook_register(session, "/nonexistent/checkouts/repo/crates")
        .await?;

    assert!(body["project_path"].is_string(), "{body}");
    assert_eq!(
        harness.binding(session).await?,
        (Some(location), "operator".to_string())
    );
    harness.cleanup().await
}

#[tokio::test]
async fn hook_register_scratchpad_cwd_takes_parent_sessions_location() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let location = harness.location("Kit", "/nonexistent/checkouts/kit").await?;
    let parent = Uuid::new_v4();
    harness
        .hook_register(parent, "/nonexistent/checkouts/kit")
        .await?;
    let child = Uuid::new_v4();
    let scratch = format!("/nonexistent/.claude/.cache/tmp/claude-1000/-checkouts-kit/{parent}/scratchpad/bench");

    harness.hook_register(child, &scratch).await?;

    assert_eq!(harness.binding(child).await?.0, Some(location));
    harness.cleanup().await
}

#[tokio::test]
async fn hook_register_under_folder_of_projects_stays_unresolved() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    harness.location("Folder", "/nonexistent/projects").await?;
    harness.location("Inner", "/nonexistent/projects/inner").await?;
    let session = Uuid::new_v4();

    let body = harness
        .hook_register(session, "/nonexistent/projects/elsewhere")
        .await?;

    assert!(body["project_path"].is_null(), "{body}");
    assert_eq!(
        harness.binding(session).await?,
        (None, "unresolved".to_string())
    );
    harness.cleanup().await
}
