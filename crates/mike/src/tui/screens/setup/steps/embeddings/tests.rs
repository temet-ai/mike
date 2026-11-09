use crossterm::event::KeyCode;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::Embeddings;
use super::check::{fetch_one, has_model, ollama_models};
use super::plan::{Current, EmbedAction, EmbedInputs, plan_panel};
use crate::tui::client::ApiClient;
use crate::tui::screens::setup::facts::{Tool, ToolFact};
use crate::tui::screens::setup::harness::{Harness, key, render_step, typed};
use crate::tui::screens::setup::step::{Outcome, WizardStep};

fn ollama() -> Option<ToolFact> {
    Some(ToolFact {
        path: "/opt/homebrew/bin/ollama".into(),
        version: Some("ollama version is 0.9.0".to_string()),
    })
}

fn inputs() -> EmbedInputs {
    EmbedInputs {
        base: "http://127.0.0.1:11434".to_string(),
        ollama: None,
        models: None,
        brew: false,
        package_manager: None,
        current: Some(Current {
            enabled: false,
            url: "http://127.0.0.1:7997".to_string(),
            model: "embeddinggemma".to_string(),
        }),
    }
}

fn actions(inputs: &EmbedInputs) -> Vec<EmbedAction> {
    plan_panel(inputs)
        .choices
        .iter()
        .map(|c| c.action)
        .collect()
}

#[test]
fn model_names_match_with_or_without_a_tag() {
    let models = vec!["embeddinggemma:latest".to_string(), "llama3:8b".to_string()];
    assert!(has_model(&models, "embeddinggemma"));
    assert!(!has_model(&models, "embedding"));
    assert!(!has_model(&models, "llama"));
    assert!(has_model(&["embeddinggemma".to_string()], "embeddinggemma"));
    assert!(!has_model(&[], "embeddinggemma"));
}

#[test]
fn with_ollama_pulled_and_running_the_choice_is_to_use_it_without_a_pull_command() {
    let mut ready = inputs();
    ready.ollama = ollama();
    ready.models = Some(vec!["embeddinggemma:latest".to_string()]);
    assert_eq!(
        actions(&ready),
        vec![
            EmbedAction::UseOllama,
            EmbedAction::OtherServer,
            EmbedAction::Leave
        ]
    );
    let panel = plan_panel(&ready);
    let choice = panel.selected();
    assert!(choice.is_some_and(|c| c.recommended && c.commands.is_empty()));
    let found = panel.found.join("\n");
    assert!(
        found.contains("answers on http://127.0.0.1:11434 and has embeddinggemma"),
        "{found}"
    );
    assert!(found.contains("Search by meaning is off"), "{found}");
}

#[test]
fn ollama_not_pulled_shows_the_pull_command_and_a_stopped_server_shows_the_start_command() {
    let mut stopped = inputs();
    stopped.ollama = ollama();
    stopped.brew = true;
    let panel = plan_panel(&stopped);
    assert_eq!(
        panel.choices[0].commands,
        vec!["ollama pull embeddinggemma", "brew services start ollama"]
    );
    assert!(
        panel
            .found
            .join("\n")
            .contains("does not answer on http://127.0.0.1:11434")
    );
    assert!(
        panel.choices[0].why.contains("http://127.0.0.1:11434/v1"),
        "{}",
        panel.choices[0].why
    );
    let mut running = inputs();
    running.ollama = ollama();
    running.models = Some(Vec::new());
    let found = plan_panel(&running).found.join("\n");
    assert!(
        found.contains("embeddinggemma is not pulled yet"),
        "{found}"
    );
}

#[test]
fn without_ollama_homebrew_installs_it_after_a_yes_and_linux_prints_the_root_command() {
    let mut mac = inputs();
    mac.brew = true;
    mac.package_manager = Some(Tool::Brew);
    let panel = plan_panel(&mac);
    assert_eq!(panel.choices[0].action, EmbedAction::InstallWithBrew);
    assert_eq!(
        panel.choices[0].commands,
        vec!["brew install ollama", "brew services start ollama"]
    );
    assert!(panel.choices[0].root_commands.is_empty());
    let mut arch = inputs();
    arch.package_manager = Some(Tool::Pacman);
    let panel = plan_panel(&arch);
    assert_eq!(panel.choices[0].action, EmbedAction::PrintInstall);
    assert_eq!(
        panel.choices[0].root_commands,
        vec!["sudo pacman -S ollama"]
    );
    assert!(panel.choices[0].commands.is_empty());
    let mut debian = inputs();
    debian.package_manager = Some(Tool::Apt);
    assert_eq!(
        plan_panel(&debian).choices[0].root_commands,
        vec!["curl -fsSL https://ollama.com/install.sh | sh"]
    );
    assert!(panel.found.join("\n").contains("Ollama is not installed"));
}

#[test]
fn another_server_takes_an_address_prefilled_with_the_current_one_and_leaving_it_off_says_what_happens()
 {
    let panel = plan_panel(&inputs());
    let other = panel
        .choices
        .iter()
        .find(|c| c.action == EmbedAction::OtherServer);
    assert_eq!(
        other
            .and_then(|c| c.input.as_ref())
            .map(|i| i.value.clone()),
        Some("http://127.0.0.1:7997".to_string())
    );
    let leave = panel
        .choices
        .iter()
        .find(|c| c.action == EmbedAction::Leave);
    assert!(leave.is_some_and(|c| c.why == "Recall uses text matching instead."));
}

fn ollama_reply(count: usize, width: usize) -> Value {
    json!({
        "object": "list",
        "data": (0..count)
            .map(|i| json!({"object": "embedding", "embedding": vec![0.5_f32; width], "index": i}))
            .collect::<Vec<_>>(),
        "model": "embeddinggemma"
    })
}

async fn ollama_server(models: &[&str], width: usize) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "models": models.iter().map(|m| json!({"name": m})).collect::<Vec<_>>()
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ollama_reply(1, width)))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn the_ollama_server_is_read_through_its_tags_route() {
    let server = ollama_server(&["embeddinggemma:latest"], 768).await;
    assert_eq!(
        ollama_models(&server.uri()).await,
        Some(vec!["embeddinggemma:latest".to_string()])
    );
    assert_eq!(ollama_models("http://127.0.0.1:1").await, None);
}

#[tokio::test]
async fn one_embedding_must_have_the_width_mike_stores() {
    let right = ollama_server(&[], 768).await;
    let finding = fetch_one(&format!("{}/v1", right.uri()), "embeddinggemma").await;
    assert!(finding.passed(), "{}", finding.text);
    assert!(finding.text.contains("768 numbers"));
    let wrong = ollama_server(&[], 384).await;
    let finding = fetch_one(&format!("{}/v1", wrong.uri()), "all-minilm").await;
    assert!(!finding.passed());
    assert!(
        finding.text.contains("returned 384 numbers"),
        "{}",
        finding.text
    );
    let down = fetch_one("http://127.0.0.1:1/v1", "embeddinggemma").await;
    assert!(!down.passed());
}

async fn proxy_server(embedding_url: &str) -> MockServer {
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
        "/api/settings",
        json!({"settings": [
            {"key": "embedding.enabled", "value": false},
            {"key": "embedding.url", "value": embedding_url},
            {"key": "embedding.model", "value": "embeddinggemma"}
        ]}),
    )
    .mount(&server)
    .await;
    get(
        "/api/capabilities",
        json!({"capabilities": [
            {"id": "semantic_recall", "label": "Semantic recall", "state": "active", "detail": "", "fix": ""}
        ]}),
    )
    .mount(&server)
    .await;
    Mock::given(method("PATCH"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;
    server
}

async fn patched(server: &MockServer) -> Vec<(String, String)> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|r| r.method.as_str() == "PATCH")
        .map(|r| {
            (
                r.url.path().to_string(),
                String::from_utf8_lossy(&r.body).to_string(),
            )
        })
        .collect()
}

async fn harness_for(proxy: &MockServer) -> anyhow::Result<Harness> {
    let mut harness = Harness::new()?;
    harness.add_executable("ollama", "#!/bin/sh\necho 'ollama version is 0.9.0'\n")?;
    harness.config.inspect_listen_addr = proxy.address().to_string();
    harness.client = ApiClient::new(&harness.config);
    Ok(harness)
}

#[tokio::test]
async fn using_ollama_sets_the_three_embedding_settings_and_fetches_one_embedding()
-> anyhow::Result<()> {
    let ollama = ollama_server(&["embeddinggemma:latest"], 768).await;
    let proxy = proxy_server("http://127.0.0.1:7997").await;
    let mut harness = harness_for(&proxy).await?;
    let mut step = Embeddings::with_base(&ollama.uri());
    step.enter(&mut harness.cx()).await;
    let text = render_step(&step);
    assert!(
        text.contains("Use Ollama with embeddinggemma (recommended)"),
        "{text}"
    );
    assert_eq!(
        step.key(key(KeyCode::Enter), &mut harness.cx()).await,
        Outcome::Stay
    );
    let text = render_step(&step);
    assert!(text.contains("embeddinggemma is already pulled."), "{text}");
    assert!(
        text.contains("The running proxy accepted 3 embedding settings."),
        "{text}"
    );
    assert!(
        text.contains("Fetched one embedding of 768 numbers"),
        "{text}"
    );
    assert!(text.contains("Semantic recall: active"), "{text}");
    let writes = patched(&proxy).await;
    assert!(
        writes
            .iter()
            .any(|(p, b)| p.ends_with("embedding.url")
                && b.contains(&format!("{}/v1", ollama.uri()))),
        "{writes:?}"
    );
    assert!(
        writes
            .iter()
            .any(|(p, b)| p.ends_with("embedding.model") && b.contains("embeddinggemma")),
        "{writes:?}"
    );
    assert!(
        writes
            .iter()
            .any(|(p, b)| p.ends_with("embedding.enabled") && b.contains("true")),
        "{writes:?}"
    );
    assert_eq!(
        step.key(key(KeyCode::Enter), &mut harness.cx()).await,
        Outcome::Next
    );
    Ok(())
}

#[tokio::test]
async fn using_ollama_when_its_server_is_down_says_how_to_start_it_and_changes_nothing()
-> anyhow::Result<()> {
    let proxy = proxy_server("http://127.0.0.1:7997").await;
    let mut harness = harness_for(&proxy).await?;
    let mut step = Embeddings::with_base("http://127.0.0.1:1");
    step.enter(&mut harness.cx()).await;
    step.key(key(KeyCode::Enter), &mut harness.cx()).await;
    let text = render_step(&step);
    assert!(
        text.contains("The Ollama server does not answer on http://127.0.0.1:1"),
        "{text}"
    );
    assert!(text.contains("ollama serve"), "{text}");
    assert!(patched(&proxy).await.is_empty());
    assert_eq!(
        step.key(key(KeyCode::Enter), &mut harness.cx()).await,
        Outcome::Stay
    );
    Ok(())
}

#[tokio::test]
async fn another_server_keeps_the_model_name_and_is_checked_with_one_embedding()
-> anyhow::Result<()> {
    let other = ollama_server(&[], 768).await;
    let proxy = proxy_server("http://127.0.0.1:7997").await;
    let mut harness = harness_for(&proxy).await?;
    let mut step = Embeddings::with_base("http://127.0.0.1:1");
    step.enter(&mut harness.cx()).await;
    step.key(key(KeyCode::Down), &mut harness.cx()).await;
    step.key(key(KeyCode::Char('e')), &mut harness.cx()).await;
    for _ in 0..30 {
        step.key(key(KeyCode::Backspace), &mut harness.cx()).await;
    }
    for event in typed(&format!("{}/v1", other.uri())) {
        step.key(event, &mut harness.cx()).await;
    }
    step.key(key(KeyCode::Enter), &mut harness.cx()).await;
    step.key(key(KeyCode::Enter), &mut harness.cx()).await;
    let writes = patched(&proxy).await;
    assert!(
        writes.iter().any(|(p, _)| p.ends_with("embedding.url")),
        "{writes:?}"
    );
    assert!(
        writes.iter().any(|(p, _)| p.ends_with("embedding.enabled")),
        "{writes:?}"
    );
    assert!(
        !writes.iter().any(|(p, _)| p.ends_with("embedding.model")),
        "{writes:?}"
    );
    let text = render_step(&step);
    assert!(
        text.contains("Fetched one embedding of 768 numbers"),
        "{text}"
    );
    Ok(())
}

#[tokio::test]
async fn leaving_it_off_changes_nothing_and_continues() -> anyhow::Result<()> {
    let proxy = proxy_server("http://127.0.0.1:7997").await;
    let mut harness = harness_for(&proxy).await?;
    let mut step = Embeddings::with_base("http://127.0.0.1:1");
    step.enter(&mut harness.cx()).await;
    step.key(key(KeyCode::Char('j')), &mut harness.cx()).await;
    step.key(key(KeyCode::Char('j')), &mut harness.cx()).await;
    step.key(key(KeyCode::Enter), &mut harness.cx()).await;
    assert!(patched(&proxy).await.is_empty());
    assert_eq!(
        step.key(key(KeyCode::Enter), &mut harness.cx()).await,
        Outcome::Next
    );
    Ok(())
}
