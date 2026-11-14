use chrono::{DateTime, Utc};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::api::{Api, FleetError, refusal_message};
use super::{ListArgs, StartArgs, StatusArgs, list_text, start_text, status_text};
use crate::launch::fleet::status::FleetStatus;

fn api_for(server: &MockServer) -> Api {
    Api::new(&server.address().to_string())
}

fn status_args(fleet: &str, json: bool) -> StatusArgs {
    StatusArgs {
        fleet: fleet.to_string(),
        watch: None,
        json,
    }
}

fn a_status() -> anyhow::Result<FleetStatus> {
    let now: DateTime<Utc> =
        DateTime::parse_from_rfc3339("2026-09-29T01:30:00Z")?.with_timezone(&Utc);
    Ok(FleetStatus {
        fleet: "3f0a1c52-6d2e-4a55-9b1e-0c8d7e6f5a41".to_string(),
        slug: "fleet-workspace".to_string(),
        state: "running".to_string(),
        run_id: None,
        goal: "ship it".to_string(),
        started_at: None,
        finished_at: None,
        now,
        missing: Vec::new(),
        criteria: Vec::new(),
        tasks: Vec::new(),
        seats: Vec::new(),
        verdicts: Vec::new(),
        blocked: Vec::new(),
        needs_operator: Vec::new(),
        accepted: false,
        obligations: vec![],
        every_seat_idle_seconds: None,
        tree_broken: None,
        problems: Vec::new(),
    })
}

#[tokio::test]
async fn status_json_prints_the_body_as_the_api_answered() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/fleets/my-fleet/status"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{\"slug\":\"x\"}\n"))
        .mount(&server)
        .await;
    let text = status_text(&api_for(&server), &status_args("my-fleet", true)).await?;
    assert_eq!(text, "{\"slug\":\"x\"}");
    Ok(())
}

#[tokio::test]
async fn status_draws_the_frame_from_the_status_the_api_answered() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/fleets/my-fleet/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(a_status()?))
        .mount(&server)
        .await;
    let text = status_text(&api_for(&server), &status_args("my-fleet", false)).await?;
    assert!(text.contains("FLEET  fleet-workspace  running"), "{text}");
    assert!(text.contains("nothing judged yet"), "{text}");
    Ok(())
}

#[tokio::test]
async fn status_of_an_unknown_fleet_is_refused_with_the_api_message() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "error": "fleet_not_found",
            "message": "there is no fleet called nope",
        })))
        .mount(&server)
        .await;
    let Err(error) = status_text(&api_for(&server), &status_args("nope", false)).await else {
        anyhow::bail!("expected the call to be refused");
    };
    assert!(matches!(error, FleetError::Refused(_)));
    assert_eq!(error.to_string(), "there is no fleet called nope");
    Ok(())
}

#[tokio::test]
async fn start_prints_the_state_the_api_answered_with() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/fleets/my-fleet/start"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "fleet": "3f0a1c52-6d2e-4a55-9b1e-0c8d7e6f5a41",
            "slug": "my-fleet",
            "state": "starting",
            "run_id": "fleet-my-fleet-3f0a1c52",
        })))
        .mount(&server)
        .await;
    let args = StartArgs {
        fleet: "my-fleet".to_string(),
    };
    let text = start_text(&api_for(&server), &args).await?;
    assert_eq!(text, "my-fleet: starting (run fleet-my-fleet-3f0a1c52)");
    Ok(())
}

#[tokio::test]
async fn start_of_a_fleet_that_is_not_ready_is_refused_with_its_message() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(409).set_body_json(serde_json::json!({
            "error": "fleet_not_ready",
            "message": "the fleet is draft; it starts once every check passes",
        })))
        .mount(&server)
        .await;
    let args = StartArgs {
        fleet: "x".to_string(),
    };
    let Err(error) = start_text(&api_for(&server), &args).await else {
        anyhow::bail!("expected the start to be refused");
    };
    assert_eq!(
        error.to_string(),
        "the fleet is draft; it starts once every check passes"
    );
    Ok(())
}

#[tokio::test]
async fn list_sends_the_project_and_limit_and_draws_a_line_per_fleet() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/fleets"))
        .and(query_param("project_path", "/p"))
        .and(query_param("limit", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "fleets": [{
                "fleet": "3f0a1c52-6d2e-4a55-9b1e-0c8d7e6f5a41",
                "slug": "my-fleet",
                "project_path": "/p",
                "state": "ready",
                "run_id": null,
                "missing": [],
                "ready": true,
                "created_at": "2026-09-28T19:30:00Z",
                "updated_at": "2026-09-28T19:31:00Z",
                "ready_at": "2026-09-28T19:31:00Z",
                "started_at": null,
                "finished_at": null,
            }],
        })))
        .mount(&server)
        .await;
    let args = ListArgs {
        project: Some("/p".to_string()),
        json: false,
    };
    let text = list_text(&api_for(&server), &args).await?;
    assert!(
        text.contains("my-fleet") && text.contains("ready") && text.contains("no run yet"),
        "{text}"
    );
    Ok(())
}

#[tokio::test]
async fn an_unreachable_api_is_reported_with_the_address_tried() -> anyhow::Result<()> {
    let Err(error) = status_text(&Api::new("127.0.0.1:1"), &status_args("x", false)).await else {
        anyhow::bail!("expected the call to fail");
    };
    assert!(matches!(error, FleetError::Unreachable { .. }));
    let message = error.to_string();
    assert!(
        message.starts_with("could not reach the inspect API at 127.0.0.1:1"),
        "{message}"
    );
    Ok(())
}

#[test]
fn a_fleet_name_with_reserved_characters_stays_one_path_segment() -> anyhow::Result<()> {
    let url = Api::new("127.0.0.1:9989").url(&["a b/c?d", "status"])?;
    assert_eq!(url.path(), "/api/fleets/a%20b%2Fc%3Fd/status");
    assert_eq!(url.query(), None);
    Ok(())
}

#[test]
fn a_refusal_names_the_message_then_the_detail_then_the_error_then_the_body() {
    assert_eq!(refusal_message(409, r#"{"error":"e","message":"m"}"#), "m");
    assert_eq!(refusal_message(409, r#"{"error":"e","detail":"d"}"#), "d");
    assert_eq!(refusal_message(409, r#"{"error":"e"}"#), "e");
    assert_eq!(refusal_message(502, "bad gateway\n"), "bad gateway");
    assert_eq!(refusal_message(500, ""), "the inspect API answered 500");
}
