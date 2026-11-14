use serde_json::json;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::pending::pending_preset;
use super::plan::{LESSONS_SETTING, Plan, Preset, RECOMMENDED_ON, default_off_keys, plan};
use super::{PENDING_KEY, apply_or_save, apply_via_api};
use crate::config::ProxyConfig;
use crate::feature_toggles::TOGGLE_META;
use crate::tui::client::ApiClient;

fn wanted(plan: &Plan, key: &str) -> Option<bool> {
    plan.toggles
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, w)| *w)
}

#[test]
fn watch_only_leaves_every_switch_at_its_default() {
    let watch = plan(Preset::WatchOnly);
    assert_eq!(watch.toggles.len(), TOGGLE_META.len());
    for key in default_off_keys() {
        assert_eq!(wanted(&watch, key), Some(false), "{key}");
    }
    assert_eq!(wanted(&watch, "harvester"), Some(true));
    assert!(!watch.session_lessons);
}

#[test]
fn recommended_adds_recall_lessons_reminders_and_code_findings_to_the_defaults_and_nothing_else() {
    let recommended = plan(Preset::Recommended);
    let watch = plan(Preset::WatchOnly);
    let extra: Vec<&str> = recommended
        .toggles
        .iter()
        .filter(|(key, on)| *on && wanted(&watch, key) == Some(false))
        .map(|(key, _)| *key)
        .collect();
    let mut expected = RECOMMENDED_ON.to_vec();
    expected.push("change_record");
    expected.sort_unstable();
    let mut got = extra;
    got.sort_unstable();
    assert_eq!(got, expected);
    assert!(recommended.session_lessons);
    for key in [
        "steerer",
        "judge_worker",
        "session_judge",
        "turn_guard",
        "proxy_tools",
    ] {
        assert_eq!(wanted(&recommended, key), Some(false), "{key}");
    }
}

#[test]
fn the_recommended_switches_exist_in_the_switch_table() {
    for key in RECOMMENDED_ON {
        assert!(TOGGLE_META.iter().any(|m| m.key == key), "{key}");
    }
}

#[test]
fn everything_turns_on_every_switch() {
    let everything = plan(Preset::Everything);
    assert!(everything.toggles.iter().all(|(_, on)| *on));
    assert!(everything.session_lessons);
}

#[test]
fn presets_parse_by_their_command_line_names() {
    for preset in [Preset::WatchOnly, Preset::Recommended, Preset::Everything] {
        assert_eq!(Preset::parse(preset.as_str()), Some(preset));
    }
    assert_eq!(Preset::parse(" Watch_Only "), Some(Preset::WatchOnly));
    assert_eq!(Preset::parse("most"), None);
    assert_eq!(pending_preset(None), None);
    assert_eq!(pending_preset(Some("everything")), Some(Preset::Everything));
}

fn client_for(server: &MockServer) -> ApiClient {
    let mut config = ProxyConfig::for_tests_in_memory();
    config.inspect_listen_addr = server.address().to_string();
    ApiClient::new(&config)
}

fn states(plan: &Plan, differ: Option<&str>) -> serde_json::Value {
    let features: Vec<serde_json::Value> = plan
        .toggles
        .iter()
        .map(|(key, on)| {
            let enabled = if Some(*key) == differ { !*on } else { *on };
            json!({"key": key, "enabled": enabled, "scope": "global", "applies": "immediate"})
        })
        .collect();
    json!({ "features": features })
}

async fn healthy_proxy(plan: &Plan, differ: Option<&str>) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path_regex(r"^/api/settings/features/[a-z_]+$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path(format!("/api/settings/{LESSONS_SETTING}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/settings/features"))
        .respond_with(ResponseTemplate::new(200).set_body_json(states(plan, differ)))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn applying_through_the_proxy_sends_every_switch_and_reads_them_back() -> anyhow::Result<()> {
    let chosen = plan(Preset::Recommended);
    let server = healthy_proxy(&chosen, None).await;
    let findings = apply_via_api(&client_for(&server), &chosen).await;
    assert!(crate::finding::all_passed(&findings), "{findings:?}");
    let requests = server.received_requests().await.unwrap_or_default();
    let patches = requests
        .iter()
        .filter(|r| r.method.as_str() == "PATCH")
        .count();
    assert_eq!(patches, chosen.toggles.len() + 1);
    let lessons = requests
        .iter()
        .find(|r| r.url.path().ends_with(LESSONS_SETTING))
        .map(|r| String::from_utf8_lossy(&r.body).to_string())
        .unwrap_or_default();
    assert!(lessons.contains("true"), "{lessons}");
    assert!(findings[1].text.contains("all "), "{findings:?}");
    Ok(())
}

#[tokio::test]
async fn a_switch_that_reads_back_differently_fails_the_check_and_is_named() -> anyhow::Result<()> {
    let chosen = plan(Preset::Recommended);
    let server = healthy_proxy(&chosen, Some("recall_injector")).await;
    let findings = apply_via_api(&client_for(&server), &chosen).await;
    let last = findings.last().map(|f| (f.passed(), f.text.clone()));
    assert_eq!(last.as_ref().map(|l| l.0), Some(false), "{findings:?}");
    assert!(last.is_some_and(|l| l.1.contains("recall_injector")));
    Ok(())
}

#[tokio::test]
async fn a_proxy_that_refuses_a_switch_has_its_reason_shown() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .respond_with(ResponseTemplate::new(503).set_body_json(
            json!({"error": "session persistence is disabled; feature toggle mutation is not available"}),
        ))
        .mount(&server)
        .await;
    let findings = apply_via_api(&client_for(&server), &plan(Preset::Everything)).await;
    assert!(findings.iter().all(|f| !f.passed()));
    assert!(
        findings[0].text.contains("session persistence is disabled"),
        "{findings:?}"
    );
    Ok(())
}

#[tokio::test]
async fn with_no_proxy_running_the_choice_is_saved_in_the_config_file() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("mike").join("config.toml");
    let mut config = ProxyConfig::for_tests_in_memory();
    config.inspect_listen_addr = "127.0.0.1:1".to_string();
    let findings = apply_or_save(&ApiClient::new(&config), &file, Preset::Recommended).await;
    assert!(crate::finding::all_passed(&findings), "{findings:?}");
    assert!(
        findings[0]
            .text
            .contains("applied when the proxy next starts")
    );
    let text = std::fs::read_to_string(&file)?;
    assert_eq!(text, format!("{PENDING_KEY} = \"recommended\"\n"));
    Ok(())
}

#[tokio::test]
async fn with_no_proxy_and_an_unwritable_config_file_the_failure_is_reported() -> anyhow::Result<()>
{
    let dir = tempfile::tempdir()?;
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, "x")?;
    let mut config = ProxyConfig::for_tests_in_memory();
    config.inspect_listen_addr = "127.0.0.1:1".to_string();
    let findings = apply_or_save(
        &ApiClient::new(&config),
        &blocker.join("config.toml"),
        Preset::Everything,
    )
    .await;
    assert!(!findings[0].passed(), "{findings:?}");
    assert!(findings[0].text.contains("could not be saved"));
    Ok(())
}
