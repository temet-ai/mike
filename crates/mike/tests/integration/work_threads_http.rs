use serde_json::{Value, json};
use uuid::Uuid;

use crate::test_db::TestDb;

use mike::config::ProxyConfig;
use mike::state::AppState;
use mike::store::persistence::PersistHandle;

struct Served {
    client: reqwest::Client,
    base: String,
    project: String,
    _server: tokio::task::JoinHandle<()>,
}

impl Served {
    async fn start(db: &crate::test_db::TestDbHandle) -> anyhow::Result<Self> {
        let mut state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
        let handle = PersistHandle::connect(&db.database_url).await?;
        state.persist = Some(handle);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let router = mike::app::inspect_router_on(state, listener.local_addr()?);
        let base = format!("http://{}", listener.local_addr()?);
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        let client = reqwest::Client::new();
        let project: Value = client
            .post(format!("{base}/api/knowledge/projects"))
            .json(&json!({"name": "Thread API project"}))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let project = project["project_uuid"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("project_uuid missing: {project}"))?
            .to_owned();
        Ok(Self {
            client,
            base,
            project,
            _server: server,
        })
    }

    async fn send(
        &self,
        request: reqwest::RequestBuilder,
    ) -> anyhow::Result<(reqwest::StatusCode, Value)> {
        let response = request.send().await?;
        let status = response.status();
        let text = response.text().await?;
        Ok((
            status,
            serde_json::from_str(&text).unwrap_or(Value::String(text)),
        ))
    }
}

struct Seeded {
    api: Served,
    session: Uuid,
    _db: crate::test_db::TestDbHandle,
}

async fn seeded() -> anyhow::Result<Option<Seeded>> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(None);
    };
    let db = factory.acquire_isolated().await?;
    let api = Served::start(&db).await?;
    let session = Uuid::new_v4();
    let (status, created) = api
        .send(
            api.client
                .post(format!("{}/api/work-threads", api.base))
                .json(&json!({
                    "project": api.project, "slug": "http-thread", "title": "HTTP thread",
                    "tldr": "first summary", "where_we_are": "at the start", "next_up": "build it",
                    "files": ["crates/mike/src/main.rs"], "session_uuid": session,
                    "goal": "prove the routes",
                })),
        )
        .await?;
    assert!(status.is_success(), "{status}: {created}");
    assert_eq!(created["created"], true);
    Ok(Some(Seeded {
        api,
        session,
        _db: db,
    }))
}

#[tokio::test]
async fn review_is_refused_with_a_message_until_criteria_are_sent() -> anyhow::Result<()> {
    let Some(s) = seeded().await? else {
        return Ok(());
    };
    let api = &s.api;
    let url = format!("{}/api/work-threads/http-thread", api.base);
    let (status, refused) = api
        .send(
            api.client
                .patch(&url)
                .json(&json!({"project": api.project, "status": "review"})),
        )
        .await?;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "{refused}");
    assert!(
        refused["error"]
            .as_str()
            .is_some_and(|m| m.contains("acceptance criteria")),
        "{refused}"
    );
    let (status, patched) = api
        .send(api.client.patch(&url).json(&json!({
            "project": api.project, "status": "review", "acceptance_criteria": "A1. it responds",
        })))
        .await?;
    assert!(status.is_success(), "{status}: {patched}");
    assert_eq!(patched["status"], "review");
    Ok(())
}

#[tokio::test]
async fn entries_are_listed_and_read_back_newest_first() -> anyhow::Result<()> {
    let Some(s) = seeded().await? else {
        return Ok(());
    };
    let api = &s.api;
    let (status, entry) = api
        .send(
            api.client
                .post(format!("{}/api/work-threads/http-thread/entries", api.base))
                .json(&json!({
                    "project": api.project, "kind": "handoff", "body": "Pick up at the export.",
                    "session_uuid": s.session,
                })),
        )
        .await?;
    assert!(status.is_success(), "{status}: {entry}");
    assert_eq!(entry["entry"]["ord"], 2);

    let (_, listed) = api
        .send(
            api.client
                .get(format!("{}/api/work-threads", api.base))
                .query(&[("project", api.project.as_str()), ("status", "active")]),
        )
        .await?;
    assert_eq!(listed["total"], 1);
    assert_eq!(listed["threads"][0]["slug"], "http-thread");
    assert_eq!(listed["threads"][0]["tldr"], "first summary");

    let (_, detail) = api
        .send(
            api.client
                .get(format!("{}/api/work-threads/http-thread", api.base))
                .query(&[("project", api.project.as_str()), ("history_limit", "1")]),
        )
        .await?;
    assert_eq!(detail["history"].as_array().map(Vec::len), Some(1));
    assert_eq!(detail["history"][0]["kind"], "handoff");
    assert_eq!(detail["history_total"], 2);
    assert_eq!(detail["state"]["files"][0], "crates/mike/src/main.rs");
    Ok(())
}

#[tokio::test]
async fn a_thread_exports_as_markdown() -> anyhow::Result<()> {
    let Some(s) = seeded().await? else {
        return Ok(());
    };
    let api = &s.api;
    let export = api
        .client
        .get(format!("{}/api/work-threads/http-thread/export", api.base))
        .query(&[("project", api.project.as_str())])
        .send()
        .await?;
    assert!(export.status().is_success());
    let content_type = export
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    assert!(content_type.starts_with("text/markdown"), "{content_type}");
    let markdown = export.text().await?;
    assert!(markdown.starts_with("# HTTP thread\n"), "{markdown}");
    assert!(markdown.contains("prove the routes") && markdown.contains("first summary"));
    Ok(())
}

#[tokio::test]
async fn unknown_threads_and_projects_are_refused_with_the_reason() -> anyhow::Result<()> {
    let Some(s) = seeded().await? else {
        return Ok(());
    };
    let api = &s.api;
    let (status, missing) = api
        .send(
            api.client
                .get(format!("{}/api/work-threads/nope", api.base))
                .query(&[("project", api.project.as_str())]),
        )
        .await?;
    assert_eq!(status, reqwest::StatusCode::NOT_FOUND, "{missing}");
    let (status, no_project) = api
        .send(api.client.get(format!("{}/api/work-threads", api.base)))
        .await?;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "{no_project}");
    let (status, bad_project) = api
        .send(
            api.client
                .get(format!("{}/api/work-threads", api.base))
                .query(&[("project", "/not/registered")]),
        )
        .await?;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "{bad_project}");
    assert!(
        bad_project["error"]
            .as_str()
            .is_some_and(|m| m.contains("/not/registered"))
    );
    Ok(())
}
