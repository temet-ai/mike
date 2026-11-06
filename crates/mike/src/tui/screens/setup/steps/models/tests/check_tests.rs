use super::*;

fn client_for(server: &MockServer) -> ApiClient {
    let mut config = crate::config::ProxyConfig::for_tests_in_memory();
    config.inspect_listen_addr = server.address().to_string();
    ApiClient::new(&config)
}

#[tokio::test]
async fn settings_are_sent_one_by_one_and_a_refusal_is_shown_with_its_reason() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/api/settings/llm.provider"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/api/settings/judge_worker.model"))
        .respond_with(
            ResponseTemplate::new(422).set_body_json(json!({"error": "model is not on the list"})),
        )
        .mount(&server)
        .await;
    let settings = vec![
        ("llm.provider".to_string(), json!("claude")),
        ("judge_worker.model".to_string(), json!("claude/nope")),
    ];
    let findings = send_settings(&client_for(&server), &settings).await;
    assert!(!all_passed(&findings));
    assert!(
        findings[0]
            .text
            .contains("judge_worker.model: model is not on the list"),
        "{findings:?}"
    );
    assert!(findings.iter().any(|f| f.text.contains("stay saved")));
}

#[tokio::test]
async fn reading_the_settings_back_compares_numbers_by_value() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/settings"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"settings": [
                {"key": "llm.daily_spend_ceiling_usd", "value": 5},
                {"key": "llm.provider", "value": "codex"}
            ]})),
        )
        .mount(&server)
        .await;
    let ok = vec![
        ("llm.daily_spend_ceiling_usd".to_string(), json!(5.0)),
        ("llm.provider".to_string(), json!("codex")),
    ];
    assert!(read_back(&client_for(&server), &ok).await.passed());
    let bad = vec![("llm.provider".to_string(), json!("claude"))];
    let finding = read_back(&client_for(&server), &bad).await;
    assert!(!finding.passed());
    assert!(finding.text.contains("llm.provider"), "{}", finding.text);
}

#[tokio::test]
async fn the_health_rows_are_reported_with_their_state_detail_and_fix() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/capabilities"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"capabilities": [
            {"id": "steering", "label": "Steering", "state": "active", "detail": "d", "fix": ""},
            {"id": "llm_role_health", "label": "LLM worker roles", "state": "inert", "detail": "Codex is not signed in.", "fix": "Run codex login."},
            {"id": "persistence", "label": "Persistence", "state": "active", "detail": "", "fix": ""}
        ]})))
        .mount(&server)
        .await;
    let rows = health_rows(&client_for(&server)).await;
    assert_eq!(rows.len(), 2, "only the model rows: {rows:?}");
    assert!(rows[0].passed() && rows[0].text == "Steering: active");
    assert!(!rows[1].passed());
    assert!(
        rows[1]
            .text
            .contains("inert. Codex is not signed in. Run codex login."),
        "{}",
        rows[1].text
    );
}

async fn proxy_with_api(openrouter_available: bool) -> MockServer {
    let server = MockServer::start().await;
    let get = |p: &str, body: Value| {
        Mock::given(method("GET"))
            .and(path(p.to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
    };
    get("/api/health", json!({"status": "ok"}))
        .mount(&server)
        .await;
    get(
        "/api/settings/features",
        json!({"features": [
            {"key": "judge_worker", "enabled": true},
            {"key": "session_judge", "enabled": true},
            {"key": "steerer", "enabled": false}
        ]}),
    )
    .mount(&server)
    .await;
    let providers = serde_json::to_value(
        payload(openrouter_available)
            .providers
            .iter()
            .map(|p| json!({
                "id": p.id, "label": p.label, "prefix": p.prefix, "available": p.available,
                "detail": p.detail, "fix": p.fix, "key_env_var": p.key_env_var,
                "models_authoritative": false,
                "models": p.models.iter().map(|m| json!({"id": m.id, "ref": m.model_ref, "label": m.label, "known_good": m.known_good})).collect::<Vec<_>>(),
                "models_error": null
            }))
            .collect::<Vec<_>>(),
    )
    .unwrap_or_default();
    get(
        "/api/llm/providers",
        json!({"llm_provider": "per_role", "providers": providers}),
    )
    .mount(&server)
    .await;
    let settings: Vec<Value> = values()
        .iter()
        .map(|(k, v)| json!({"key": k, "value": v}))
        .collect();
    get("/api/settings", json!({"settings": settings}))
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path_regex(r"^/api/settings/.+$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn the_step_walks_from_the_provider_choice_to_a_saved_and_checked_configuration()
-> anyhow::Result<()> {
    let server = proxy_with_api(false).await;
    let mut harness = Harness::new()?;
    harness.config.inspect_listen_addr = server.address().to_string();
    harness.client = ApiClient::new(&harness.config);
    let mut step = Models::new();
    step.enter(&mut harness.cx()).await;
    let text = render_step(&step);
    assert!(
        text.contains("Use Claude for every job (recommended)"),
        "{text}"
    );
    assert_eq!(
        step.key(key(KeyCode::Enter), &mut harness.cx()).await,
        Outcome::Stay
    );
    let text = render_step(&step);
    assert!(text.contains("Provider for every job"), "{text}");
    assert!(text.contains("Used by: Turn judge"), "{text}");
    for _ in 0..7 {
        step.key(key(KeyCode::Down), &mut harness.cx()).await;
    }
    assert_eq!(
        step.key(key(KeyCode::Enter), &mut harness.cx()).await,
        Outcome::Stay
    );
    let text = render_step(&step);
    assert!(
        text.contains("Sent 7 settings to the running proxy."),
        "{text}"
    );
    assert!(
        text.contains("No test call was made to claude")
            || text.contains("One small test call to claude/"),
        "{text}"
    );
    let patched: Vec<String> = server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|r| r.method.as_str() == "PATCH")
        .map(|r| r.url.path().to_string())
        .collect();
    assert!(
        patched.contains(&"/api/settings/llm.provider".to_string()),
        "{patched:?}"
    );
    assert!(
        patched.contains(&"/api/settings/llm.provider_models.claude.fast".to_string()),
        "{patched:?}"
    );
    Ok(())
}

#[tokio::test]
async fn with_no_proxy_the_step_says_how_to_start_it_and_continues_on_enter() -> anyhow::Result<()>
{
    let mut harness = Harness::new()?;
    let mut step = Models::new();
    step.enter(&mut harness.cx()).await;
    let text = render_step(&step);
    assert!(
        text.contains("The proxy is not running, so the models cannot be set now"),
        "{text}"
    );
    assert_eq!(
        step.key(key(KeyCode::Enter), &mut harness.cx()).await,
        Outcome::Next
    );
    Ok(())
}
