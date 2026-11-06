use crossterm::event::KeyCode;
use serde_json::json;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::super::harness::{Harness, key, render_step};
use super::super::step::{Outcome, WizardStep};
use super::super::steps::features::{FeatureInputs, Features, plan_panel};
use crate::feature_presets::{Preset, plan};
use crate::tui::client::ApiClient;

fn inputs(switched_on: &[&str]) -> FeatureInputs {
    FeatureInputs {
        proxy_answers: true,
        persistent: true,
        switched_on: switched_on.iter().map(|s| (*s).to_string()).collect(),
    }
}

#[test]
fn the_cursor_starts_on_recommended_among_the_three_choices() {
    let panel = plan_panel(&inputs(&[]));
    let presets: Vec<Preset> = panel.choices.iter().map(|c| c.action).collect();
    assert_eq!(
        presets,
        vec![Preset::WatchOnly, Preset::Recommended, Preset::Everything]
    );
    assert_eq!(
        panel.selected().map(|c| c.action),
        Some(Preset::Recommended)
    );
    assert_eq!(panel.choices.iter().filter(|c| c.recommended).count(), 1);
    assert_eq!(panel.choices[1].commands, vec!["mike features recommended"]);
}

#[test]
fn the_choices_explain_themselves_in_plain_words() {
    let panel = plan_panel(&inputs(&[]));
    assert!(
        panel.choices[1]
            .why
            .contains("recall, lessons and reminders")
    );
    assert!(panel.choices[1].why.contains("nothing leaves the machine"));
    assert!(panel.choices[2].why.contains("OpenRouter"));
    assert!(panel.choices[2].why.contains(&format!(
        "All {} switches",
        plan(Preset::Everything).toggles.len()
    )));
}

#[test]
fn a_choice_names_the_switches_it_would_turn_off() {
    let panel = plan_panel(&inputs(&["steerer", "recall_injector"]));
    assert!(
        panel.choices[0]
            .why
            .contains("turns off these switches that are on now: steerer, recall_injector"),
        "{}",
        panel.choices[0].why
    );
    assert!(
        panel.choices[1]
            .why
            .contains("turns off these switches that are on now: steerer."),
        "{}",
        panel.choices[1].why
    );
    assert!(
        !panel.choices[2].why.contains("turns off"),
        "{}",
        panel.choices[2].why
    );
    let quiet = plan_panel(&inputs(&[]));
    assert!(quiet.choices.iter().all(|c| !c.why.contains("turns off")));
}

#[test]
fn the_found_lines_say_whether_the_proxy_answers_and_what_is_switched_on() {
    let down = FeatureInputs {
        proxy_answers: false,
        persistent: false,
        switched_on: Vec::new(),
    };
    let found = plan_panel(&down).found.join("\n");
    assert!(
        found.contains("Nothing answers on the inspect address"),
        "{found}"
    );
    assert!(found.contains("memory only"), "{found}");
    let found = plan_panel(&inputs(&["steerer"])).found.join("\n");
    assert!(
        found.contains("Switched on now beyond the defaults: steerer"),
        "{found}"
    );
}

async fn proxy(patch_status: u16) -> MockServer {
    let server = MockServer::start().await;
    let chosen = plan(Preset::Recommended);
    let states: Vec<serde_json::Value> = chosen
        .toggles
        .iter()
        .map(|(k, on)| json!({"key": k, "enabled": on}))
        .collect();
    Mock::given(method("GET"))
        .and(path("/api/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/settings/features"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"features": states})))
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path_regex(r"^/api/settings/.+$"))
        .respond_with(ResponseTemplate::new(patch_status).set_body_json(json!({"error": "no"})))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn running_recommended_applies_it_through_the_proxy_and_checks_it_by_reading_back()
-> anyhow::Result<()> {
    let server = proxy(200).await;
    let mut harness = Harness::new()?;
    harness.config.persist_enabled = true;
    harness.config.inspect_listen_addr = server.address().to_string();
    harness.client = ApiClient::new(&harness.config);
    let mut step = Features::new();
    step.enter(&mut harness.cx()).await;
    assert_eq!(
        step.key(key(KeyCode::Enter), &mut harness.cx()).await,
        Outcome::Stay
    );
    let text = render_step(&step);
    assert!(
        text.contains("Read the switches back from the proxy"),
        "{text}"
    );
    assert!(
        text.contains("all 33 match the choice") || text.contains("match the choice"),
        "{text}"
    );
    assert_eq!(
        step.key(key(KeyCode::Enter), &mut harness.cx()).await,
        Outcome::Next
    );
    Ok(())
}

#[tokio::test]
async fn a_refusal_is_shown_and_the_step_does_not_continue() -> anyhow::Result<()> {
    let server = proxy(503).await;
    let mut harness = Harness::new()?;
    harness.config.persist_enabled = true;
    harness.config.inspect_listen_addr = server.address().to_string();
    harness.client = ApiClient::new(&harness.config);
    let mut step = Features::new();
    step.enter(&mut harness.cx()).await;
    step.key(key(KeyCode::Enter), &mut harness.cx()).await;
    let text = render_step(&step);
    assert!(text.contains("The proxy refused"), "{text}");
    assert_eq!(
        step.key(key(KeyCode::Enter), &mut harness.cx()).await,
        Outcome::Stay
    );
    Ok(())
}

#[tokio::test]
async fn with_no_proxy_the_choice_is_saved_for_the_next_start() -> anyhow::Result<()> {
    let mut harness = Harness::new()?;
    harness.config.persist_enabled = true;
    let mut step = Features::new();
    step.enter(&mut harness.cx()).await;
    step.key(key(KeyCode::Enter), &mut harness.cx()).await;
    assert!(
        harness
            .config_text()
            .contains("setup_features = \"recommended\""),
        "{}",
        harness.config_text()
    );
    let text = render_step(&step);
    assert!(
        text.contains("applied when the proxy next") && text.contains("starts."),
        "{text}"
    );
    Ok(())
}

#[tokio::test]
async fn in_memory_mode_changes_nothing_and_says_why() -> anyhow::Result<()> {
    let mut harness = Harness::new()?;
    harness.config.persist_enabled = false;
    let mut step = Features::new();
    step.enter(&mut harness.cx()).await;
    step.key(key(KeyCode::Enter), &mut harness.cx()).await;
    assert_eq!(harness.config_text(), "");
    let text = render_step(&step);
    assert!(text.contains("keeps no switch settings"), "{text}");
    Ok(())
}
