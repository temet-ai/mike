use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use mike::launch::fleet::introduce;
use mike::store::persistence::PersistHandle;
use mike::store::persistence::launch::{LaunchSurfaceRefs, NewLaunch, NewRun};

use crate::seed::{SeedRequest, seed_project, seed_session};
use crate::test_db::TestDb;

const RUN: &str = "fleet-introductions-1";

struct Ran<'a> {
    seat: &'a str,
    request_id: i64,
    command: &'a str,
    started: DateTime<Utc>,
    seconds: i64,
    background: bool,
}

async fn seat_ran(handle: &PersistHandle, pool: &PgPool, ran: Ran<'_>) -> anyhow::Result<Uuid> {
    let session = Uuid::new_v4();
    let project = seed_project(pool, "/tmp/introductions").await?;
    seed_session(pool, project, &session.to_string()).await?;
    let mut launch = NewLaunch::new("herdr", "/tmp/introductions");
    launch.session_uuid = Some(session);
    launch.run_id = Some(RUN.to_owned());
    launch.seat = Some(ran.seat.to_owned());
    let launch_id = handle.insert_launch(&launch).await?;
    handle
        .record_launch_opened(launch_id, &LaunchSurfaceRefs::default())
        .await?;
    SeedRequest::new(ran.request_id, project)
        .session_uuid(&session.to_string())
        .insert(pool)
        .await?;
    sqlx::query(
        "INSERT INTO responses.responses (request_id, response_status, stop_reason_id, completed_at)
         SELECT $1, 200, sr.id, $2
         FROM responses.stop_reasons sr
         WHERE sr.code = 'tool_use'",
    )
    .bind(ran.request_id)
    .bind(ran.started)
    .execute(pool)
    .await?;
    sqlx::query(
        "WITH used AS (
             INSERT INTO tools.uses (wire_tool_use_id, request_id, ord, tool_name, input, session_id)
             SELECT $1, $2, 0, 'Bash', $3, s.id
               FROM sessions.sessions s
              WHERE s.session_uuid = $4
             RETURNING id, session_id
         )
         INSERT INTO tools.results (request_id, ord, tool_use_id, session_id, is_error, content,
                                    content_sha256, content_bytes, created_at)
         SELECT $2, 0, used.id, used.session_id, false, '\"ok\"'::jsonb, 'sha', 2, $5
           FROM used",
    )
    .bind(format!("toolu_{}", ran.request_id))
    .bind(ran.request_id)
    .bind(serde_json::json!({"command": ran.command, "run_in_background": ran.background}))
    .bind(session)
    .bind(ran.started + Duration::seconds(ran.seconds))
    .execute(pool)
    .await?;
    Ok(session)
}

async fn seed_five_commands(
    handle: &PersistHandle,
    pool: &PgPool,
    start: DateTime<Utc>,
) -> anyhow::Result<(Uuid, Uuid)> {
    let rows = [
        ("py-dev-T1", 81_001, "make  test\n  -k slow", 0, 60, false),
        ("py-dev-T2", 81_002, "make test -k slow", 5, 45, false),
        ("py-dev-T3", 81_003, "git status", 6, 1, false),
        ("coordinator", 81_004, "make test -k slow", 7, 45, false),
        ("py-dev-T3", 81_005, "make test -k slow", 8, 0, true),
    ];
    let mut sessions = Vec::new();
    for (seat, request_id, command, minute, seconds, background) in rows {
        let ran = Ran {
            seat,
            request_id,
            command,
            started: start + Duration::minutes(minute),
            seconds,
            background,
        };
        sessions.push(seat_ran(handle, pool, ran).await?);
    }
    Ok((sessions[0], sessions[1]))
}

#[tokio::test]
async fn slow_commands_of_working_seats_are_read_and_a_repeat_is_recorded_once()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = handle.pool();
    handle
        .upsert_run(&NewRun {
            run_id: RUN.to_owned(),
            project_path: Some("/tmp/introductions".to_owned()),
            backend: "herdr".to_owned(),
            workspace_ref: None,
            tab_ref: None,
        })
        .await?;
    let start = Utc::now() - Duration::minutes(30);
    let (first, again) = seed_five_commands(&handle, pool, start).await?;

    let since = start - Duration::minutes(1);
    let slow = handle.slow_commands(RUN, since, 20.0).await?;
    let seats: Vec<(&str, &str)> = slow
        .iter()
        .map(|r| (r.seat.as_str(), r.command.as_str()))
        .collect();
    assert_eq!(
        seats,
        vec![
            ("py-dev-T1", "make  test\n  -k slow"),
            ("py-dev-T2", "make test -k slow"),
            ("py-dev-T3", "make test -k slow")
        ],
        "slow and backgrounded commands of working seats are read, oldest first, whole"
    );
    assert!(slow[2].in_background && !slow[1].in_background);
    assert_eq!(slow[0].session, first);
    assert_eq!(slow[1].session, again);
    assert!(!slow[0].is_error);
    assert_eq!((slow[1].finished_at - slow[1].started_at).num_seconds(), 45);

    assert!(
        introduce::repeats(&slow, &[]).is_empty(),
        "capture latency is not authoritative command timing"
    );
    let results = handle.tree_results(RUN, Some("make"), since, 20).await?;
    let first_result = results
        .iter()
        .find(|row| row.use_id == slow[0].use_id)
        .ok_or_else(|| {
            anyhow::anyhow!("captured result disappeared when introduction was skipped")
        })?;
    assert_eq!(first_result.content, Some(serde_json::json!("ok")));
    assert_eq!(
        first_result.command.as_deref(),
        Some("make  test\n  -k slow")
    );
    assert_eq!(first_result.session_uuid, Some(first));
    assert_eq!(first_result.is_error, Some(false));
    assert!(
        handle
            .recorded_introductions(&[slow[1].use_id])
            .await?
            .is_empty()
    );

    let pair = [(slow[1].use_id, slow[0].use_id)];
    assert_eq!(
        handle.record_introductions(RUN, &pair).await?,
        vec![slow[1].use_id]
    );
    assert!(
        handle.record_introductions(RUN, &pair).await?.is_empty(),
        "a repeat is recorded once"
    );
    assert!(handle.record_introductions(RUN, &[]).await?.is_empty());
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_closed_first_seats_captured_result_is_readable_without_contacting_it()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    handle
        .upsert_run(&NewRun {
            run_id: RUN.to_owned(),
            project_path: Some("/tmp/introductions".to_owned()),
            backend: "herdr".to_owned(),
            workspace_ref: None,
            tab_ref: None,
        })
        .await?;
    let start = Utc::now() - Duration::minutes(30);
    let work = "cargo clippy -p gg-sessions --all-features";
    let first = seat_ran(
        &handle,
        handle.pool(),
        Ran {
            seat: "sessions-T1",
            request_id: 82_001,
            command: work,
            started: start,
            seconds: 170,
            background: false,
        },
    )
    .await?;
    seat_ran(
        &handle,
        handle.pool(),
        Ran {
            seat: "sessions-T11",
            request_id: 82_002,
            command: work,
            started: start + Duration::minutes(5),
            seconds: 185,
            background: false,
        },
    )
    .await?;
    let launch = handle
        .find_launch_by_session(first)
        .await?
        .ok_or_else(|| anyhow::anyhow!("first launch missing"))?;
    handle
        .record_launch_closed(launch.id, "complete", Some(true))
        .await?;
    let since = start - Duration::minutes(1);
    let slow = handle.slow_commands(RUN, since, 5.0).await?;
    assert!(introduce::repeats(&slow, &[]).is_empty());
    let results = handle
        .tree_results(RUN, Some("cargo clippy"), since, 20)
        .await?;
    let captured = results
        .iter()
        .find(|row| row.session_uuid == Some(first))
        .ok_or_else(|| anyhow::anyhow!("closed seat result unavailable"))?;
    assert_eq!(captured.content, Some(serde_json::json!("ok")));
    let timings = [
        introduce::CommandTiming {
            use_id: slow[0].use_id,
            work: work.into(),
            seconds: 170.0,
        },
        introduce::CommandTiming {
            use_id: slow[1].use_id,
            work: work.into(),
            seconds: 185.0,
        },
    ];
    let measured = introduce::repeats_with_timings(&slow, &[], &timings);
    assert_eq!(measured.len(), 1);
    let mail = introduce::mail(&measured[0], &[]);
    assert!(mail.contains("No contact with the first seat is required"));
    assert!(!mail.contains("Mail both"));
    assert!(mail.contains("list_session_tool_uses"));
    assert!(mail.contains(&format!("\"find_use_id\":{}", captured.use_id)));
    assert!(mail.contains("\"commit\":null"));
    assert!(mail.contains("\"still_valid\":null"));
    db.cleanup().await?;
    Ok(())
}

async fn results_endpoint(
    handle: &PersistHandle,
) -> anyhow::Result<(String, Uuid, tokio::task::JoinHandle<std::io::Result<()>>)> {
    let fleet = handle
        .insert_fleet(&mike::store::persistence::fleet::NewFleet {
            slug: format!("raw-results-{}", Uuid::new_v4()),
            project_path: "/tmp/introductions".into(),
            definition: serde_json::json!({}),
            missing: serde_json::json!([]),
            created_by: None,
        })
        .await?;
    for state in ["ready", "starting", "running"] {
        let run = (state == "starting").then_some(RUN);
        handle
            .set_fleet_state(fleet.fleet_uuid, state, run, None)
            .await?;
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let mut config = mike::config::ProxyConfig::for_tests_in_memory();
    config.inspect_listen_addr = listener.local_addr()?.to_string();
    let mut state = mike::state::AppState::new(config).await?;
    state.persist = Some(handle.clone());
    let router = mike::api::manifest::inspect_router(state);
    let url = format!(
        "http://{}/api/fleets/{}/results",
        listener.local_addr()?,
        fleet.fleet_uuid
    );
    let server = tokio::spawn(async move { axum::serve(listener, router).await });
    Ok((url, fleet.fleet_uuid, server))
}

#[tokio::test]
async fn results_route_raw_capture_is_unknown_and_cross_crate_changes_during_execution_invalidate()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    handle
        .upsert_run(&NewRun {
            run_id: RUN.into(),
            project_path: Some("/tmp/introductions".into()),
            backend: "herdr".into(),
            workspace_ref: None,
            tab_ref: None,
        })
        .await?;
    let work = "cargo build --bin gg";
    seat_ran(
        &handle,
        handle.pool(),
        Ran {
            seat: "ink-T7",
            request_id: 83_001,
            command: work,
            started: Utc::now() - Duration::minutes(10),
            seconds: 60,
            background: false,
        },
    )
    .await?;
    let (url, fleet, server) = results_endpoint(&handle).await?;
    let client = reqwest::Client::new();
    let filtered = format!("{url}?command=cargo%20build&paths=crates/gg-ui&include_output=true");
    let before = request_results(&client, &filtered).await?;
    assert_eq!(
        before["results"][0].get("still_valid"),
        Some(&serde_json::Value::Null)
    );
    assert_eq!(before["results"][0]["is_error"], false);
    assert_eq!(before["results"][0]["output_tail"], "ok");
    assert!(before["results"][0]["use_id"].is_number());
    assert!(
        before["validity_provenance"]
            .as_str()
            .is_some_and(|p| p.contains("no commit binding"))
    );
    handle
        .insert_tree_changes(
            RUN,
            &[mike::store::persistence::fleet::NewTreeChange {
                path: "crates/gg-core/src/host.rs".into(),
                change: "modified".into(),
                content_sha: None,
            }],
        )
        .await?;
    seat_ran(
        &handle,
        handle.pool(),
        Ran {
            seat: "core-T9",
            request_id: 83_002,
            command: work,
            started: Utc::now() - Duration::seconds(60),
            seconds: 60,
            background: false,
        },
    )
    .await?;
    finish_fixture_fleet(&handle, fleet).await?;
    let after = request_results(&client, &filtered).await?;
    let rows = after["results"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("result rows missing"))?;
    assert_eq!(rows.len(), 2);
    for row in rows {
        assert_eq!(row["still_valid"], false);
        assert!(row.get("changed_since").is_none());
        assert_eq!(row["output_tail"], "ok");
    }
    let all = request_results(&client, &url).await?;
    assert_eq!(
        all["results"][0]["changed_since"],
        serde_json::json!(["crates/gg-core/src/host.rs"])
    );
    server.abort();
    let _ = server.await;
    db.cleanup().await?;
    Ok(())
}

async fn request_results(client: &reqwest::Client, url: &str) -> anyhow::Result<serde_json::Value> {
    Ok(client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

async fn finish_fixture_fleet(handle: &PersistHandle, fleet: Uuid) -> anyhow::Result<()> {
    let stopping = handle
        .set_fleet_state(fleet, "stopping", None, None)
        .await?;
    for launch in handle.unsettled_fleet_launches(RUN).await? {
        handle
            .record_fleet_launch_verified_exit(launch.id, RUN, "fixture complete")
            .await?;
    }
    let done = handle
        .complete_fleet_run(&stopping, None)
        .await?
        .ok_or_else(|| anyhow::anyhow!("fixture fleet did not complete"))?;
    assert_eq!(done.state, "done");
    Ok(())
}

#[path = "introduction_contracts.rs"]
mod contracts;
