mod seed;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use serde_json::Value;
use sqlx::PgPool;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;
use uuid::Uuid;

use crate::test_db::TestDb;

use mike::api::manifest::inspect_router;
use mike::config::ProxyConfig;
use mike::state::AppState;
use seed::{insert_lesson, seed_delivery_story, seed_session};

struct Env {
    db: mike::store::test_support::TestDbHandle,
    pool: PgPool,
    router: Router,
    shutdown: CancellationToken,
}

impl Env {
    async fn boot() -> anyhow::Result<Option<Self>> {
        let Some(factory) = TestDb::bootstrap().await? else {
            return Ok(None);
        };
        let db = factory.acquire_isolated().await?;
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
        let pool = state
            .persist()
            .map(|persist| persist.pool().clone())
            .ok_or_else(|| anyhow::anyhow!("persist_enabled config must yield a PersistHandle"))?;
        let shutdown = state.shutdown();
        let router = inspect_router(state);
        Ok(Some(Self {
            db,
            pool,
            router,
            shutdown,
        }))
    }

    async fn cleanup(self) -> anyhow::Result<()> {
        self.shutdown.cancel();
        self.db.cleanup().await
    }

    async fn get_delivery(&self, id: &str) -> anyhow::Result<(StatusCode, Value)> {
        let response = self
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .header("host", "127.0.0.1:0")
                    .uri(format!("/api/sessions/{id}/delivery"))
                    .body(Body::empty())?,
            )
            .await?;
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await?;
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes)?
        };
        Ok((status, body))
    }
}

fn list<'a>(value: &'a Value, field: &str) -> anyhow::Result<&'a Vec<Value>> {
    value[field]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("{field} is not an array in {value}"))
}

fn entry<'a>(items: &'a [Value], key: &str, wanted: &str) -> anyhow::Result<&'a Value> {
    items
        .iter()
        .find(|item| item[key] == wanted)
        .ok_or_else(|| anyhow::anyhow!("no entry with {key}={wanted}"))
}

fn has_topic(body: &Value, name: &str) -> bool {
    body["topics"]
        .as_array()
        .is_some_and(|topics| topics.iter().any(|topic| topic["topic"] == name))
}

fn gap_codes(body: &Value) -> Vec<String> {
    body["evidence_gaps"]
        .as_array()
        .map(|gaps| {
            gaps.iter()
                .filter_map(|gap| gap["code"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn delivery_reports_profile_toggles_and_delivered_lessons() -> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        return Ok(());
    };
    let seeded = seed_delivery_story(&env.pool).await?;

    let (status, body) = env.get_delivery(&seeded.session.to_string()).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["session_uuid"], seeded.session.to_string());
    assert_eq!(body["profile"]["project"]["path"], "/work/delivery-api");
    assert_eq!(body["profile"]["models"][0]["request_count"], 3);
    assert_eq!(body["profile"]["models"][0]["requests_with_manifest"], 2);

    let recall = entry(list(&body["profile"], "toggles")?, "key", "recall_injector")?;
    assert_eq!(recall["enabled"], false);
    assert_eq!(recall["enabled_now"], false);
    assert_eq!(recall["changed_since_session_start"], true);
    let change = entry(
        list(&body["profile"], "toggle_changes")?,
        "key",
        "recall_injector",
    )?;
    assert_eq!(change["enabled_before"], false);
    assert_eq!(change["enabled_after"], true);
    assert_eq!(change["during_session"], false);

    let lessons = list(&body, "lessons")?;
    assert_eq!(lessons.len(), 1);
    assert_eq!(lessons[0]["text"], "Use scripts/cx for cargo.");
    assert_eq!(lessons[0]["injection_count"], 1);
    assert_eq!(lessons[0]["lanes"][0], "primary");
    assert_eq!(lessons[0]["personas"][0], "Rust Development Expert");
    assert_eq!(lessons[0]["origins"][0], "proxy");

    env.cleanup().await
}

#[tokio::test]
async fn delivery_reports_topics_skills_stages_and_gaps() -> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        return Ok(());
    };
    let seeded = seed_delivery_story(&env.pool).await?;

    let (_, body) = env.get_delivery(&seeded.session.to_string()).await?;
    let topics = list(&body, "topics")?;
    let sqlx = entry(topics, "topic", "sqlx")?;
    assert_eq!(sqlx["state"], "recalled");
    assert_eq!(sqlx["available_project_lessons"], 1);
    assert_eq!(sqlx["available_shared_lessons"], 1);
    assert_eq!(sqlx["recall_request_ids"].as_array().map(Vec::len), Some(1));
    assert_eq!(entry(topics, "topic", "tokio")?["state"], "available");
    assert!(
        !has_topic(&body, "axum"),
        "archived lessons are not available topics"
    );
    assert_eq!(body["recall_injections"][0]["lesson_count"], 2);

    let skills = list(&body, "skills")?;
    assert_eq!(skills.len(), 2);
    assert_eq!(entry(skills, "name", "rust-lsp")?["state"], "loaded");
    assert_eq!(entry(skills, "name", "missing")?["state"], "load_failed");
    assert_eq!(
        body["recall_tool_calls"][0]["tool_name"],
        "mcp__mike__recall_project_context"
    );

    let stages = list(&body, "injection_stages")?;
    assert!(stages.iter().any(|stage| {
        stage["stage"] == "recall_injector"
            && stage["outcome"] == "applied"
            && stage["request_count"] == 1
    }));
    let codes = gap_codes(&body);
    assert!(codes.contains(&"requests_without_manifest".to_owned()));
    assert!(codes.contains(&"skill_selection_not_recorded".to_owned()));
    assert!(codes.contains(&"session_settings_not_in_memory".to_owned()));
    assert!(!codes.contains(&"lesson_reminders_unattributed".to_owned()));

    env.cleanup().await
}

#[tokio::test]
async fn delivery_for_session_without_requests_is_empty_not_missing() -> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        return Ok(());
    };
    let seeded = seed_session(&env.pool, "/work/delivery-empty").await?;

    let (status, body) = env.get_delivery(&seeded.session.to_string()).await?;
    assert_eq!(status, StatusCode::OK);
    for field in ["lessons", "skills", "topics", "injection_stages"] {
        assert!(list(&body, field)?.is_empty(), "{field} should be empty");
    }

    env.cleanup().await
}

#[tokio::test]
async fn delivery_keeps_two_projects_apart() -> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        return Ok(());
    };
    let first = seed_session(&env.pool, "/work/delivery-one").await?;
    let second = seed_session(&env.pool, "/work/delivery-two").await?;
    insert_lesson(
        &env.pool,
        Some(first.project_id),
        "one only",
        Some("only-one"),
        "daily",
    )
    .await?;
    insert_lesson(
        &env.pool,
        Some(second.project_id),
        "two only",
        Some("only-two"),
        "daily",
    )
    .await?;

    let (_, first_body) = env.get_delivery(&first.session.to_string()).await?;
    let (_, second_body) = env.get_delivery(&second.session.to_string()).await?;
    assert!(has_topic(&first_body, "only-one"));
    assert!(!has_topic(&first_body, "only-two"));
    assert!(has_topic(&second_body, "only-two"));
    assert!(!has_topic(&second_body, "only-one"));

    env.cleanup().await
}

#[tokio::test]
async fn delivery_for_unknown_or_malformed_session_is_not_found() -> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        return Ok(());
    };

    let (unknown, _) = env.get_delivery(&Uuid::new_v4().to_string()).await?;
    assert_eq!(unknown, StatusCode::NOT_FOUND);
    let (malformed, _) = env.get_delivery("not-a-uuid").await?;
    assert_eq!(malformed, StatusCode::NOT_FOUND);

    env.cleanup().await
}
