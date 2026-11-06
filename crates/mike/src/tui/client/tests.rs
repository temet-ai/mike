use super::*;

#[tokio::test]
async fn tcp_reachable_is_false_for_a_dead_port() {
    assert!(!tcp_reachable("127.0.0.1:1").await);
}

#[tokio::test]
async fn tcp_reachable_is_true_for_a_bound_listener() -> anyhow::Result<()> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?.to_string();
    assert!(tcp_reachable(&addr).await);
    Ok(())
}

fn client_against(mock_addr: std::net::SocketAddr) -> ApiClient {
    let mut config = crate::config::ProxyConfig::for_tests_in_memory();
    config.inspect_listen_addr = mock_addr.to_string();
    ApiClient::new(&config)
}

#[tokio::test]
async fn capabilities_parses_the_servers_real_response_shape() -> anyhow::Result<()> {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/api/capabilities"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "capabilities": [
                {"id": "steering", "label": "Steering & judges", "state": "inert", "detail": "no key", "fix": "set a key"},
                {"id": "persistence", "label": "Persistence", "state": "active", "detail": "connected", "fix": ""}
            ]
        })))
        .mount(&server)
        .await;

    let client = client_against(server.address().to_owned());
    let Some(payload) = client.capabilities().await else {
        anyhow::bail!("must parse the mocked response");
    };
    assert_eq!(payload.capabilities.len(), 2);
    assert_eq!(payload.capabilities[0].state, CapabilityState::Inert);
    assert_eq!(payload.capabilities[1].state, CapabilityState::Active);
    Ok(())
}

#[tokio::test]
async fn settings_parses_the_servers_real_response_shape() -> anyhow::Result<()> {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/api/settings"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "settings": [{
                    "key": "embedding.url",
                    "group": "Embedding",
                    "kind": "string",
                    "scope": "global",
                    "applies": "on_restart",
                    "default": "http://127.0.0.1:7997",
                    "label": "Embedding URL",
                    "description": "Base URL of the Infinity embedding server.",
                    "value": "http://127.0.0.1:7997"
                }],
                "steerer_openrouter_configured_via_env": true
            })),
        )
        .mount(&server)
        .await;

    let client = client_against(server.address().to_owned());
    let Some(payload) = client.settings().await else {
        anyhow::bail!("must parse the mocked response");
    };
    assert!(payload.steerer_openrouter_configured_via_env);
    assert_eq!(payload.settings[0].key, "embedding.url");
    assert_eq!(payload.settings[0].kind, SettingKind::String);
    assert_eq!(payload.settings[0].applies, SettingApplies::OnRestart);
    Ok(())
}

#[tokio::test]
async fn list_sessions_parses_the_servers_real_response_shape() -> anyhow::Result<()> {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/api/sessions"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                "id": "aaaaaaaa-0000-0000-0000-000000000001",
                "first_seen": "2026-08-10T10:00:00Z",
                "last_seen": "2026-08-16T09:00:00Z",
                "request_count": 3,
                "models": ["claude-sonnet-5"],
                "last_status": "Complete",
                "project": "temet-proxy",
                "project_id": 7,
                "cwd": "/Users/x/temet-proxy",
                "project_resolution": "git",
                "git_repo": "temet-proxy",
                "pinned": false,
                "ttl_override": null,
                "busy": false,
                "title": "fix the thing",
                "avg_latency_ms": 812.5,
                "originating_operator_prompt": "fix the thing please",
                "session_outcome": "success",
                "session_quality_score": 0.9,
                "session_judged_at": "2026-08-10T11:00:00Z",
                "narrative_overview": null,
                "narrative_overview_generated_at": null,
                "pending_inbox_count": 0
            }])),
        )
        .mount(&server)
        .await;

    let client = client_against(server.address().to_owned());
    let Some(sessions) = client.list_sessions(0, 50).await else {
        anyhow::bail!("must parse the mocked response");
    };
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, "aaaaaaaa-0000-0000-0000-000000000001");
    assert_eq!(sessions[0].first_seen, "2026-08-10T10:00:00Z");
    assert_eq!(sessions[0].title.as_deref(), Some("fix the thing"));
    assert!(!sessions[0].busy);
    Ok(())
}

#[tokio::test]
async fn session_requests_parses_the_servers_real_response_shape() -> anyhow::Result<()> {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/api/sessions/aaaaaaaa-0000-0000-0000-000000000001/requests",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "session_id": "aaaaaaaa-0000-0000-0000-000000000001",
                "requests": [{
                    "id": 42,
                    "session_id": "aaaaaaaa-0000-0000-0000-000000000001",
                    "method": "POST",
                    "path": "/v1/messages",
                    "model": "claude-sonnet-5",
                    "status": "Complete",
                    "is_streaming": true,
                    "message_count": 3,
                    "max_tokens": null,
                    "created_at": "2026-08-16T09:00:00Z",
                    "completed_at": "2026-08-16T09:00:05Z",
                    "project": "temet-proxy",
                    "git_repo": "temet-proxy",
                    "human_prompt": "fix the thing",
                    "turn_type": "user",
                    "turn_origin": "primary",
                    "tool_names": ["Read", "Edit"],
                    "response_tool_count": 1,
                    "response_tools_used": ["Edit"]
                }],
                "total_requests": 1,
                "offset": 0,
                "limit": 50
            })),
        )
        .mount(&server)
        .await;

    let client = client_against(server.address().to_owned());
    let Some(page) = client
        .session_requests("aaaaaaaa-0000-0000-0000-000000000001", 0, 50)
        .await
    else {
        anyhow::bail!("must parse the mocked response");
    };
    assert_eq!(page.total_requests, 1);
    assert_eq!(page.requests.len(), 1);
    assert_eq!(page.requests[0].id, 42);
    assert_eq!(page.requests[0].status, "Complete");
    assert_eq!(page.requests[0].tool_names, vec!["Read", "Edit"]);
    Ok(())
}

#[tokio::test]
async fn request_detail_parses_complete_status_from_a_bare_string() -> anyhow::Result<()> {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/api/requests/42"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 42,
                "upstream_request_id": "req_abc",
                "upstream_message_id": "msg_abc",
                "session_id": "aaaaaaaa-0000-0000-0000-000000000001",
                "request_number": 3,
                "method": "POST",
                "path": "/v1/messages",
                "request_headers": [["content-type", "application/json"]],
                "request_body_size": 1024,
                "model": "claude-sonnet-5",
                "is_streaming": true,
                "message_count": 3,
                "is_tool_loop": false,
                "last_user_message": "fix the thing",
                "tool_names": ["Read"],
                "params": {
                    "max_tokens": 4096,
                    "temperature": null,
                    "thinking_enabled": false,
                    "thinking_budget": null,
                    "thinking_mode": null
                },
                "project": "temet-proxy",
                "git_repo": "temet-proxy",
                "human_prompt": "fix the thing\nacross two lines",
                "turn_type": "user",
                "turn_origin": "primary",
                "status": "Complete",
                "response_status": 200,
                "response_headers": [["content-type", "text/event-stream"]],
                "response_body_size": 2048,
                "tool_calls": [],
                "token_usage": {
                    "input_tokens": 100,
                    "output_tokens": 50,
                    "cache_creation_input_tokens": null,
                    "cache_read_input_tokens": 10
                },
                "stop_reason": "end_turn",
                "stop_details": null,
                "created_at": "2026-08-16T09:00:00Z",
                "completed_at": "2026-08-16T09:00:05Z",
                "diagnostics": [],
                "composition_applied": false,
                "unknown_blocks": null,
                "system_blocks": null,
                "filter_modified": false,
                "firing_rule_ids": []
            })),
        )
        .mount(&server)
        .await;

    let client = client_against(server.address().to_owned());
    let Some(detail) = client.request_detail(42).await else {
        anyhow::bail!("must parse the mocked response");
    };
    assert_eq!(detail.status, RequestStatusDto::Complete);
    assert_eq!(detail.request_number, Some(3));
    assert_eq!(detail.human_prompt, "fix the thing\nacross two lines");
    let Some(usage) = detail.token_usage else {
        anyhow::bail!("token_usage must decode");
    };
    assert_eq!(usage.input_tokens, 100);
    assert_eq!(usage.cache_read_input_tokens, Some(10));
    Ok(())
}

#[tokio::test]
async fn request_detail_parses_error_status_from_a_tagged_object() -> anyhow::Result<()> {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/api/requests/43"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 43,
                "upstream_request_id": null,
                "upstream_message_id": null,
                "session_id": "aaaaaaaa-0000-0000-0000-000000000001",
                "method": "POST",
                "path": "/v1/messages",
                "request_headers": [],
                "request_body_size": 512,
                "model": "claude-sonnet-5",
                "is_streaming": false,
                "message_count": 1,
                "is_tool_loop": false,
                "last_user_message": "boom",
                "tool_names": [],
                "params": {
                    "max_tokens": null,
                    "temperature": null,
                    "thinking_enabled": false,
                    "thinking_budget": null,
                    "thinking_mode": null
                },
                "project": null,
                "git_repo": null,
                "human_prompt": "boom",
                "turn_type": "user",
                "turn_origin": "primary",
                "status": {"Error": "upstream 529"},
                "response_status": 529,
                "response_headers": null,
                "response_body_size": 0,
                "tool_calls": [],
                "token_usage": null,
                "stop_reason": null,
                "stop_details": null,
                "created_at": "2026-08-16T09:00:00Z",
                "completed_at": null,
                "diagnostics": [],
                "composition_applied": false,
                "unknown_blocks": null,
                "system_blocks": null,
                "filter_modified": false,
                "firing_rule_ids": []
            })),
        )
        .mount(&server)
        .await;

    let client = client_against(server.address().to_owned());
    let Some(detail) = client.request_detail(43).await else {
        anyhow::bail!("must parse the mocked response");
    };
    assert_eq!(
        detail.status,
        RequestStatusDto::Error("upstream 529".to_string())
    );
    assert_eq!(detail.status.to_string(), "error: upstream 529");
    Ok(())
}

#[tokio::test]
async fn request_body_parses_the_servers_real_response_shape() -> anyhow::Result<()> {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/api/requests/42/body/response"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "text": "event: message_start\ndata: {}\n\n",
                "byte_size": 33,
                "has_body": true
            })),
        )
        .mount(&server)
        .await;

    let client = client_against(server.address().to_owned());
    let Some(body) = client.request_body(42, BodyKind::Response).await else {
        anyhow::bail!("must parse the mocked response");
    };
    assert!(body.has_body);
    assert_eq!(body.byte_size, 33);
    assert_eq!(
        body.text.as_deref(),
        Some("event: message_start\ndata: {}\n\n")
    );
    Ok(())
}

#[tokio::test]
async fn patch_setting_surfaces_the_servers_error_message() -> anyhow::Result<()> {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("PATCH"))
        .and(wiremock::matchers::path("/api/settings/embedding.url"))
        .respond_with(
            wiremock::ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": "invalid value for setting 'embedding.url': too long"
            })),
        )
        .mount(&server)
        .await;

    let client = client_against(server.address().to_owned());
    let result = client
        .patch_setting("embedding.url", serde_json::json!("x"))
        .await;
    let Err(err) = result else {
        anyhow::bail!("a 400 response must surface as Err");
    };
    assert!(err.contains("too long"), "{err}");
    Ok(())
}
