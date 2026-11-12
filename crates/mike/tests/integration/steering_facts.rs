use chrono::{DateTime, Duration, Utc};
use mike::dossier::steering_facts::wait_backing::WaitBacking;
use mike::dossier::steering_facts::{STEERING_FACTS_VERSION, SteeringFacts, fill};
use mike::store::persistence::steering_facts::StaleFilter;

use crate::turn_facts::resolution::build;
use crate::turn_facts::{Fixture, NewTurn, Req, attach, fixture, insert_request, insert_turn};

fn at(f: &Fixture, secs: i64) -> DateTime<Utc> {
    f.base + Duration::seconds(secs)
}

async fn seed_turn(f: &Fixture) -> anyhow::Result<i64> {
    insert_request(f, &Req::primary(9500, 0, "fix the parser and test it")).await?;
    insert_request(f, &Req::lane(9501, "primary", 5)).await?;
    insert_request(f, &Req::lane(9502, "primary", 25)).await?;
    let turn = insert_turn(f, &NewTurn::closed(9500, 9502, 0, 50)).await?;
    attach(f, turn, &[9500, 9501, 9502]).await?;
    Ok(turn)
}

async fn tool_call(
    f: &Fixture,
    request_id: i64,
    tool: &str,
    input: serde_json::Value,
    secs: i64,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO tools.uses (request_id, session_id, ord, wire_tool_use_id, tool_name, input, created_at)
         VALUES ($1, $2, 0, 'toolu_' || gen_random_uuid(), $3, $4, $5)
         RETURNING id",
    )
    .bind(request_id)
    .bind(f.session_id)
    .bind(tool)
    .bind(input)
    .bind(at(f, secs))
    .fetch_one(f.handle.pool())
    .await?;
    sqlx::query(
        "INSERT INTO tools.results
             (request_id, session_id, tool_use_id, ord, is_error, content, content_sha256, content_bytes, created_at)
         VALUES ($1, $2, $3, 0, false, to_jsonb('ok'::text), 'sha', 2, $4)",
    )
    .bind(request_id)
    .bind(f.session_id)
    .bind(id)
    .bind(at(f, secs + 1))
    .execute(f.handle.pool())
    .await?;
    Ok(id)
}

async fn launch_work_item(f: &Fixture, secs: i64) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO agents.work_items
             (session_id, wire_tool_use_id, kind, task_id, launched_at, state)
         VALUES ($1, 'toolu_' || gen_random_uuid(), 'bash', 'task-late', $2, 'running')
         RETURNING id",
    )
    .bind(f.session_id)
    .bind(at(f, secs))
    .fetch_one(f.handle.pool())
    .await?)
}

#[tokio::test]
async fn as_of_before_a_later_backing_event_ignores_that_event() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = seed_turn(&f).await?;
    let item = launch_work_item(&f, 60).await?;
    let before = SteeringFacts::as_of(&f.handle, f.session_id, turn, at(&f, 55)).await?;
    assert_eq!(before.wait_backing, WaitBacking::None);
    assert!(before.wait_backing_refs.is_empty());
    let after = SteeringFacts::as_of(&f.handle, f.session_id, turn, at(&f, 70)).await?;
    assert_eq!(after.wait_backing, WaitBacking::WorkItem);
    assert_eq!(after.wait_backing_refs[0].id, Some(item));
    assert_eq!(after.version, STEERING_FACTS_VERSION);
    Ok(())
}

#[tokio::test]
async fn observation_as_of_reads_only_calls_made_by_then() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = seed_turn(&f).await?;
    let edit = tool_call(
        &f,
        9501,
        "Edit",
        serde_json::json!({"file_path": "src/parser.rs"}),
        10,
    )
    .await?;
    let test = tool_call(
        &f,
        9502,
        "Bash",
        serde_json::json!({"command": "cargo nextest run"}),
        30,
    )
    .await?;
    let mid = SteeringFacts::as_of(&f.handle, f.session_id, turn, at(&f, 20)).await?;
    assert_eq!(mid.observation.observed_after_change, Some(false));
    assert_eq!(mid.observation.last_change_use_id, Some(edit));
    let end = SteeringFacts::as_of(&f.handle, f.session_id, turn, at(&f, 50)).await?;
    assert_eq!(end.observation.observed_after_change, Some(true));
    assert_eq!(end.observation.last_observation_use_id, Some(test));
    assert_eq!(end.observation.observation_kind, Some("test"));
    Ok(())
}

#[tokio::test]
async fn fill_writes_closed_turns_once_and_a_rerun_writes_nothing() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = seed_turn(&f).await?;
    tool_call(
        &f,
        9501,
        "Edit",
        serde_json::json!({"file_path": "src/parser.rs"}),
        10,
    )
    .await?;
    build(&f).await?;
    let filter = StaleFilter {
        session_ids: vec![f.session_id],
        ..StaleFilter::default()
    };
    let dry = fill(&f.handle, filter.clone(), false).await?;
    assert_eq!(dry.written, 0);
    let rerun = fill(&f.handle, filter, true).await?;
    assert_eq!(rerun.written, 0, "the build already filled the closed turn");
    let persisted = f.handle.persisted_steering_facts(turn).await?;
    let Some(row) = persisted else {
        anyhow::bail!("turn {turn} has no turn_facts row");
    };
    assert_eq!(row.steering_facts_version, STEERING_FACTS_VERSION);
    assert_eq!(row.wait_backing.as_deref(), Some("none"));
    assert_eq!(row.observed_after_change, Some(false));
    Ok(())
}

async fn serve(f: &Fixture) -> anyhow::Result<String> {
    let args = vec![
        "mike".to_string(),
        "--inspect-listen-addr".to_string(),
        "127.0.0.1:0".to_string(),
        "--persist-enabled".to_string(),
        "1".to_string(),
        "--database-url".to_string(),
        f.db.database_url.clone(),
    ];
    let state =
        mike::state::AppState::new(mike::config::ProxyConfig::try_parse_from(args)?).await?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let base = format!("http://{}", listener.local_addr()?);
    let router = mike::app::inspect_router_on(state, listener.local_addr()?);
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    Ok(base)
}

#[tokio::test]
async fn rest_view_returns_the_three_facts_with_evidence_as_of_t() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = seed_turn(&f).await?;
    let edit = tool_call(
        &f,
        9501,
        "Edit",
        serde_json::json!({"file_path": "src/parser.rs"}),
        10,
    )
    .await?;
    let item = launch_work_item(&f, 60).await?;
    let uuid: String =
        sqlx::query_scalar("SELECT session_uuid::text FROM sessions.sessions WHERE id = $1")
            .bind(f.session_id)
            .fetch_one(f.handle.pool())
            .await?;
    let base = serve(&f).await?;
    let as_of = at(&f, 70).to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
    let url = format!("{base}/api/sessions/{uuid}/turns/{turn}/steering-facts");
    let response = reqwest::Client::new()
        .get(&url)
        .query(&[("as_of", as_of.as_str())])
        .send()
        .await?;
    let status = response.status();
    let body: serde_json::Value = response.json().await?;
    anyhow::ensure!(status.is_success(), "GET {url}: {status}: {body}");
    assert_eq!(body["turn_id"], turn);
    let computed = &body["computed"];
    assert_eq!(computed["wait_backing"], "work_item");
    assert_eq!(computed["wait_backing_refs"][0]["id"], item);
    assert_eq!(computed["observation"]["observed_after_change"], false);
    assert_eq!(computed["observation"]["last_change_use_id"], edit);
    assert!(computed.get("session_class").is_some(), "{body}");
    assert!(body.get("persisted").is_some(), "{body}");
    let missing = reqwest::get(format!(
        "{base}/api/sessions/00000000-0000-4000-8000-00000000dead/turns/{turn}/steering-facts"
    ))
    .await?;
    assert_eq!(missing.status(), reqwest::StatusCode::NOT_FOUND);
    let bad = reqwest::get(format!("{url}?as_of=yesterday")).await?;
    assert_eq!(bad.status(), reqwest::StatusCode::BAD_REQUEST);
    Ok(())
}
