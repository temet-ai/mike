//! Moved from crates/mike/src/api/handlers/project_identity_tests.rs by the crate split: it reaches the serve
//! crate, so it is tested from there.

use chrono::{DateTime, TimeZone, Utc};
use reqwest::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::config::ProxyConfig;
use crate::state::AppState;
use crate::store::persistence::listings::ProjectDetailRow;
use crate::store::persistence::rehydrate::{ProjectDetailHead, ProjectListRow, ProjectLocationRow};
use crate::store::persistence::{PersistHandle, test_support::acquire_ephemeral_db};

use crate::api::handlers::ProjectDto;

struct Inspect {
    base: String,
    client: reqwest::Client,
    server: tokio::task::JoinHandle<()>,
}

impl Inspect {
    async fn start(state: AppState) -> anyhow::Result<Self> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let router = crate::app::inspect_router_on(state, listener.local_addr()?);
        let base = format!("http://{}", listener.local_addr()?);
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        Ok(Self {
            base,
            client: reqwest::Client::new(),
            server,
        })
    }

    async fn get(&self, segments: &[&str]) -> anyhow::Result<(StatusCode, Value)> {
        let mut url = reqwest::Url::parse(&self.base)?;
        url.path_segments_mut()
            .map_err(|()| anyhow::anyhow!("base url cannot carry a path"))?
            .extend(segments);
        let response = self.client.get(url).send().await?;
        let status = response.status();
        Ok((status, response.json().await?))
    }

    async fn post(&self, route: &str, body: Value) -> anyhow::Result<Value> {
        let response = self
            .client
            .post(format!("{}{route}", self.base))
            .json(&body)
            .send()
            .await?;
        anyhow::ensure!(
            response.status().is_success(),
            "{route}: {}",
            response.status()
        );
        Ok(response.json().await?)
    }

    fn stop(self) {
        self.server.abort();
    }
}

fn at_hour(hour: u32) -> anyhow::Result<DateTime<Utc>> {
    Utc.with_ymd_and_hms(2026, 9, 21, hour, 0, 0)
        .single()
        .ok_or_else(|| anyhow::anyhow!("hour {hour} is not a valid instant"))
}

fn list_row(uuid: Uuid) -> anyhow::Result<ProjectDetailHead> {
    let at = at_hour(12)?;
    Ok(ProjectDetailHead {
        session_count: 2,
        request_count: 9,
        project: ProjectListRow {
            project_id: 7,
            project_uuid: uuid,
            project_path: "/Users/test/app".to_owned(),
            first_seen: at,
            last_seen: at,
            pinned: false,
            ttl_override: None,
            pin_created_at: None,
            ttl_override_set_at: None,
            custom_name: Some("App".to_owned()),
        },
    })
}

#[test]
fn project_dto_carries_uuid_and_nests_locations() -> anyhow::Result<()> {
    let project_uuid = Uuid::new_v4();
    let location = ProjectLocationRow {
        location_uuid: Uuid::new_v4(),
        machine_uuid: Uuid::new_v4(),
        machine_name: "Mac".to_owned(),
        root_path: "/Users/test/app".to_owned(),
        last_seen: at_hour(13)?,
    };
    let dto = ProjectDto::from(ProjectDetailRow {
        project: list_row(project_uuid)?,
        locations: vec![location],
    });
    let value = serde_json::to_value(&dto)?;
    assert_eq!(value["project_uuid"], project_uuid.to_string());
    assert_eq!(value["project_id"], 7);
    assert_eq!(value["session_count"], 2);
    assert_eq!(value["custom_name"], "App");
    assert_eq!(value["locations"][0]["machine_name"], "Mac");
    assert_eq!(value["locations"][0]["root_path"], "/Users/test/app");
    assert_eq!(
        value["locations"][0]["last_seen"],
        "2026-09-21T13:00:00+00:00"
    );
    for field in ["location_uuid", "machine_uuid"] {
        assert!(value["locations"][0][field].is_string(), "{field}: {value}");
    }
    Ok(())
}

#[tokio::test]
async fn project_lookup_without_persistence_returns_503() -> anyhow::Result<()> {
    let inspect = Inspect::start(AppState::new(ProxyConfig::for_tests_in_memory()).await?).await?;
    let by_uuid = Uuid::new_v4().to_string();
    for segments in [
        vec!["api", "projects", by_uuid.as_str()],
        vec!["api", "projects", "by-path", "/Users/test/app"],
    ] {
        let (status, body) = inspect.get(&segments).await?;
        assert_eq!(
            status,
            StatusCode::SERVICE_UNAVAILABLE,
            "{segments:?}: {body}"
        );
    }
    inspect.stop();
    Ok(())
}

#[tokio::test]
async fn project_lookup_with_non_uuid_returns_400_naming_the_input() -> anyhow::Result<()> {
    let inspect = Inspect::start(AppState::new(ProxyConfig::for_tests_in_memory()).await?).await?;
    let (status, body) = inspect.get(&["api", "projects", "not-a-uuid"]).await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["project_uuid"], "not-a-uuid");
    let message = body["error"].as_str().unwrap_or_default();
    assert!(
        message.contains("not-a-uuid") && message.contains("by-path"),
        "{body}"
    );
    inspect.stop();
    Ok(())
}

async fn define_project_with_locations(inspect: &Inspect) -> anyhow::Result<Value> {
    let project = inspect
        .post(
            "/api/knowledge/projects",
            json!({"name": "Portable project"}),
        )
        .await?;
    for (machine, root) in [("Studio", "/Users/test/app"), ("Laptop", "/home/test/app")] {
        inspect
            .post(
                "/api/knowledge/locations",
                json!({
                    "project": project["project_uuid"], "machine_uuid": Uuid::new_v4(),
                    "machine_name": machine, "root_path": root
                }),
            )
            .await?;
    }
    Ok(project)
}

fn field<'a>(value: &'a Value, name: &str) -> anyhow::Result<&'a str> {
    value[name]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("`{name}` missing from {value}"))
}

async fn assert_lookups_agree(inspect: &Inspect, project: &Value) -> anyhow::Result<()> {
    let uuid = field(project, "project_uuid")?;
    let (status, by_uuid) = inspect.get(&["api", "projects", uuid]).await?;
    assert_eq!(status, StatusCode::OK, "{by_uuid}");
    let path = format!("mike:{uuid}");
    let (status, by_path) = inspect.get(&["api", "projects", "by-path", &path]).await?;
    assert_eq!(status, StatusCode::OK, "{by_path}");
    assert_eq!(by_uuid, by_path);
    assert_eq!(by_uuid["project_uuid"], uuid);
    assert_eq!(by_uuid["project_path"], path);
    assert_eq!(by_uuid["custom_name"], "Portable project");
    assert_eq!(by_uuid["session_count"], 0);
    let machines: Vec<&str> = by_uuid["locations"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("locations missing: {by_uuid}"))?
        .iter()
        .filter_map(|l| l["machine_name"].as_str())
        .collect();
    assert_eq!(machines, ["Laptop", "Studio"]);
    let first = &by_uuid["locations"][0];
    assert_eq!(first["root_path"], "/home/test/app");
    for name in ["location_uuid", "machine_uuid", "last_seen"] {
        assert!(first[name].is_string(), "{name}: {first}");
    }
    Ok(())
}

async fn assert_list_carries_uuids(inspect: &Inspect, project: &Value) -> anyhow::Result<()> {
    let (status, list) = inspect.get(&["api", "projects"]).await?;
    assert_eq!(status, StatusCode::OK);
    let entries = list
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("expected an array: {list}"))?;
    assert!(
        entries.iter().all(|e| e["project_uuid"].is_string()),
        "every entry must carry project_uuid: {list}"
    );
    let deliberate = entries
        .iter()
        .find(|e| e["project_uuid"] == project["project_uuid"])
        .ok_or_else(|| anyhow::anyhow!("deliberately defined project missing: {list}"))?;
    assert!(deliberate.get("session_count").is_none(), "{deliberate}");
    assert!(deliberate.get("request_count").is_none(), "{deliberate}");
    let url = format!("{}/api/projects?counts=true", inspect.base);
    let counted: Value = inspect.client.get(url).send().await?.json().await?;
    let counted_entry = counted
        .as_array()
        .and_then(|all| {
            all.iter()
                .find(|e| e["project_uuid"] == project["project_uuid"])
        })
        .ok_or_else(|| anyhow::anyhow!("project missing from counts=true list: {counted}"))?;
    assert_eq!(counted_entry["session_count"], 0);
    assert_eq!(counted_entry["request_count"], 0);
    Ok(())
}

async fn assert_legacy_path_and_misses(
    inspect: &Inspect,
    handle: &PersistHandle,
) -> anyhow::Result<()> {
    let legacy = "/Users/legacy/repo";
    let project_id = handle.upsert_project_returning_id(legacy).await?;
    sqlx::query("INSERT INTO sessions.sessions(session_uuid, project_id) VALUES ($1, $2)")
        .bind(Uuid::new_v4())
        .bind(project_id)
        .execute(handle.pool())
        .await?;
    let (status, by_path) = inspect.get(&["api", "projects", "by-path", legacy]).await?;
    assert_eq!(status, StatusCode::OK, "{by_path}");
    assert_eq!(by_path["session_count"], 1);
    assert_eq!(by_path["locations"], json!([]));
    let uuid = field(&by_path, "project_uuid")?;
    let (_, by_uuid) = inspect.get(&["api", "projects", uuid]).await?;
    assert_eq!(by_uuid, by_path);

    let (status, body) = inspect
        .get(&["api", "projects", &Uuid::new_v4().to_string()])
        .await?;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert!(body["requested"].is_string(), "{body}");
    let (status, body) = inspect
        .get(&["api", "projects", "by-path", "(no project)"])
        .await?;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    Ok(())
}

#[tokio::test]
async fn project_lookups_return_one_identity_and_the_list_carries_it() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let mut state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    state.persist = Some(handle.clone());
    let inspect = Inspect::start(state).await?;
    let project = define_project_with_locations(&inspect).await?;
    assert_lookups_agree(&inspect, &project).await?;
    assert_list_carries_uuids(&inspect, &project).await?;
    assert_legacy_path_and_misses(&inspect, &handle).await?;
    inspect.stop();
    db.cleanup().await?;
    Ok(())
}
