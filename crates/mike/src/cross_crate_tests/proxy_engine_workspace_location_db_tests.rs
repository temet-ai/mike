//! Moved from crates/mike/src/proxy/engine/workspace_location_db_tests.rs by the crate split: it reaches the serve
//! crate, so it is tested from there.
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;

use std::time::{Duration, Instant};

use serde_json::json;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::config::ProxyConfig;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::project_knowledge::{LocationInput, ProjectIdentity};
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};

struct Env {
    db: TestDbHandle,
    state: AppState,
    persist: PersistHandle,
    upstream: MockServer,
    proxy_url: String,
    host: Uuid,
}

impl Env {
    async fn boot() -> anyhow::Result<Self> {
        let db = acquire_ephemeral_db()
            .await?
            .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
        let upstream = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "msg_test", "type": "message", "role": "assistant",
                "content": [{"type": "text", "text": "ok"}],
                "model": "claude-test", "stop_reason": "end_turn",
                "usage": {"input_tokens": 1, "output_tokens": 1}
            })))
            .mount(&upstream)
            .await;
        let config = ProxyConfig::try_parse_from([
            "mike",
            "--upstream-url",
            &upstream.uri(),
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
        let host = match state.host_machine_uuid.get() {
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
        let proxy_url = format!("http://{}", listener.local_addr()?);
        let router = crate::app::proxy_router(state.clone());
        tokio::spawn(async move { axum::serve(listener, router).await });
        Ok(Self {
            db,
            state,
            persist,
            upstream,
            proxy_url,
            host,
        })
    }

    async fn project(&self, name: &str) -> anyhow::Result<ProjectIdentity> {
        Ok(self
            .persist
            .knowledge()
            .define_knowledge_project(name, Uuid::new_v4())
            .await?)
    }

    async fn location(
        &self,
        project: &ProjectIdentity,
        machine: Uuid,
        root: &str,
    ) -> anyhow::Result<Uuid> {
        let row = self
            .persist
            .knowledge()
            .register_project_location(&LocationInput {
                project: project.project_uuid.to_string(),
                machine_uuid: machine,
                machine_name: "machine".into(),
                root_path: root.into(),
            })
            .await?;
        Ok(serde_json::from_value(row["location_uuid"].clone())?)
    }

    async fn first_request(&self, session: Uuid, cwd: &str) -> anyhow::Result<()> {
        let body = json!({
            "model": "claude-test",
            "max_tokens": 16,
            "system": format!(
                "Environment\n - Primary working directory: {cwd}\n - Is a git repository: true"
            ),
            "messages": [{"role": "user", "content": "hello"}],
        });
        let response = reqwest::Client::new()
            .post(format!("{}/v1/messages", self.proxy_url))
            .header("x-claude-code-session-id", session.to_string())
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .await?;
        anyhow::ensure!(
            response.status().is_success(),
            "proxy answered {}",
            response.status()
        );
        response.bytes().await?;
        Ok(())
    }

    async fn settled_registration(
        &self,
        session: Uuid,
    ) -> anyhow::Result<crate::store::persistence::workspace::RegisteredWorkspaceSession> {
        let start = Instant::now();
        loop {
            let cell = self
                .state
                .workspace_registration_cells
                .get(&session.to_string())
                .map(|entry| entry.clone());
            if let Some(registered) = cell.as_ref().and_then(|cell| cell.get()) {
                return Ok(registered.clone());
            }
            if start.elapsed() > Duration::from_secs(10) {
                anyhow::bail!(
                    "timed out waiting for the first-request registration of {session}; cell present: {}",
                    cell.is_some()
                );
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    async fn session_row(&self, session: Uuid) -> anyhow::Result<(Option<Uuid>, i64, String)> {
        Ok(sqlx::query_as(
            "SELECT location_uuid, project_id, project_resolution
             FROM sessions.sessions WHERE session_uuid = $1",
        )
        .bind(session)
        .fetch_one(self.persist.pool())
        .await?)
    }

    async fn cleanup(self) -> anyhow::Result<()> {
        self.state.shutdown().cancel();
        self.db.cleanup().await
    }
}

#[tokio::test]
async fn first_request_without_hooks_binds_the_session_to_its_local_location() -> anyhow::Result<()>
{
    let env = Env::boot().await?;
    let project = env.project("Portable").await?;
    let location = env.location(&project, env.host, "/home/dev/app").await?;
    let session = Uuid::new_v4();

    env.first_request(session, "/home/dev/app/crates/mike")
        .await?;

    let registered = env.settled_registration(session).await?;
    assert_eq!(registered.registered_via, "passive");
    assert_eq!(registered.project_resolution, "operator");
    assert_eq!(registered.project_id, project.id);
    assert_eq!(
        registered.project_path.as_deref(),
        Some(project.project_path.as_str())
    );
    assert_eq!(
        env.session_row(session).await?,
        (Some(location), project.id, "operator".to_string())
    );
    env.cleanup().await
}

#[tokio::test]
async fn first_request_outside_every_registered_root_stays_path_resolved() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let project = env.project("Elsewhere").await?;
    env.location(&project, env.host, "/home/dev/app").await?;
    env.location(&project, Uuid::new_v4(), "/srv/unrelated")
        .await?;
    let session = Uuid::new_v4();

    env.first_request(session, "/srv/unrelated/tool").await?;

    let registered = env.settled_registration(session).await?;
    assert_ne!(registered.project_resolution, "operator");
    let (bound, project_id, _) = env.session_row(session).await?;
    assert_eq!(bound, None);
    assert_ne!(project_id, project.id);
    env.cleanup().await
}

#[tokio::test]
async fn first_request_keeps_the_binding_a_launcher_registered_session_already_has()
-> anyhow::Result<()> {
    let env = Env::boot().await?;
    let launched = env.project("Launched").await?;
    let local = env.project("Local").await?;
    let launched_location = env
        .location(&launched, Uuid::new_v4(), "/Users/dev/app")
        .await?;
    env.location(&local, env.host, "/home/dev/app").await?;
    let session = Uuid::new_v4();
    env.persist
        .register_workspace_session(
            crate::store::persistence::workspace::RegisterWorkspaceSession {
                session_uuid: session,
                project_path: Some(&launched.project_path),
                cwd: "/home/dev/app",
                worktree: None,
                registered_via: "launcher",
                intent: None,
                role: None,
                name: None,
                instructions: None,
                pid: None,
            },
        )
        .await?;
    env.persist
        .knowledge()
        .bind_session_location(session, launched_location)
        .await?;

    env.first_request(session, "/home/dev/app/src").await?;

    let registered = env.settled_registration(session).await?;
    assert_eq!(registered.project_id, launched.id);
    assert_eq!(
        env.session_row(session).await?,
        (Some(launched_location), launched.id, "operator".to_string())
    );
    env.cleanup().await
}

#[tokio::test]
async fn first_request_upstream_still_receives_the_request() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let project = env.project("Forwarded").await?;
    env.location(&project, env.host, "/home/dev/app").await?;

    env.first_request(Uuid::new_v4(), "/home/dev/app").await?;

    let received = env
        .upstream
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("upstream recorded no requests"))?;
    assert_eq!(received.len(), 1);
    env.cleanup().await
}
