use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};

use super::build::providers_in;
use super::source::provider_of;
use crate::finding::Finding;
use crate::llm::{LlmRuntimeSettings, OpenRouterClient};
use crate::tui::client::{ApiClient, CapabilityState};

const PING_WAIT: Duration = Duration::from_secs(60);
const HEALTH_ROWS: [&str; 2] = ["steering", "llm_role_health"];

#[derive(Debug, Deserialize)]
struct Pong {
    ok: bool,
}

pub(super) async fn ping(model: &str) -> Result<(), String> {
    let client = OpenRouterClient::new(String::new(), model.to_string(), None, Some(PING_WAIT))
        .map_err(|e| e.to_string())?
        .with_runtime_settings(LlmRuntimeSettings {
            max_output_tokens: 64,
            record_usage: false,
            ..LlmRuntimeSettings::default()
        });
    let schema = json!({
        "type": "object",
        "properties": { "ok": { "type": "boolean" } },
        "required": ["ok"],
        "additionalProperties": false
    });
    let reply: Pong = client
        .verdict(
            "Answer with the JSON object {\"ok\": true} and nothing else.",
            "ping",
            schema,
        )
        .await
        .map_err(|e| e.to_string())?;
    if reply.ok {
        Ok(())
    } else {
        Err("the model answered ok = false".to_string())
    }
}

pub(super) async fn send_settings(
    client: &ApiClient,
    settings: &[(String, Value)],
) -> Vec<Finding> {
    let mut failures = Vec::new();
    for (key, value) in settings {
        if let Err(why) = client.patch_setting(key, value.clone()).await {
            failures.push(Finding::failed(format!("The proxy refused {key}: {why}")));
        }
    }
    if !failures.is_empty() {
        failures.push(Finding::note(
            "Settings sent before the refusal stay saved. Fix the value and save again.",
        ));
        return failures;
    }
    vec![Finding::ok(format!(
        "Sent {} settings to the running proxy.",
        settings.len()
    ))]
}

fn same(sent: &Value, stored: &Value) -> bool {
    match (sent.as_f64(), stored.as_f64()) {
        (Some(a), Some(b)) => (a - b).abs() < 1e-9,
        _ => sent == stored,
    }
}

pub(super) async fn read_back(client: &ApiClient, settings: &[(String, Value)]) -> Finding {
    let Some(stored) = client.setting_values().await else {
        return Finding::failed("The proxy did not return its settings when they were read back.");
    };
    let differing: Vec<&str> = settings
        .iter()
        .filter(|(key, sent)| !stored.get(key).is_some_and(|value| same(sent, value)))
        .map(|(key, _)| key.as_str())
        .collect();
    if differing.is_empty() {
        Finding::ok(format!(
            "Read the settings back from the proxy: all {} match.",
            settings.len()
        ))
    } else {
        Finding::failed(format!(
            "After saving, these settings differ from what was sent: {}.",
            differing.join(", ")
        ))
    }
}

fn first_model_of(settings: &[(String, Value)], provider: &str) -> Option<String> {
    settings
        .iter()
        .filter(|(key, _)| key != "llm.provider")
        .filter_map(|(_, value)| value.as_str())
        .find(|text| provider_of(text) == provider)
        .map(str::to_string)
}

async fn check_openrouter(client: &ApiClient) -> Finding {
    match client.verify_openrouter_key().await {
        Some(verdict) if verdict.valid => Finding::ok(format!(
            "OpenRouter accepted the key that the running proxy reads from {}.",
            verdict.env_var_name
        )),
        Some(verdict) if verdict.configured => Finding::failed(format!(
            "OpenRouter rejected the key that the running proxy reads from {}.",
            verdict.env_var_name
        )),
        Some(verdict) => Finding::failed(format!(
            "The running proxy has no OpenRouter key: the variable {} is empty in its environment. A background service does not see your shell's variables, so save the key in the config file instead.",
            verdict.env_var_name
        )),
        None => Finding::failed("The proxy did not answer the OpenRouter key check."),
    }
}

async fn check_provider(client: &ApiClient, provider: &str, model: Option<String>) -> Finding {
    if provider == "openrouter" {
        return check_openrouter(client).await;
    }
    if crate::sandbox::active() {
        return Finding::note(format!(
            "No test call was made to {provider}, because test calls are switched off while {} is set.",
            crate::sandbox::ENV_VAR
        ));
    }
    let Some(model) = model else {
        return Finding::note(format!("No model uses {provider}, so it was not tested."));
    };
    match ping(&model).await {
        Ok(()) => Finding::ok(format!("One small test call to {model} was answered.")),
        Err(why) => Finding::failed(format!("The test call to {model} failed: {why}")),
    }
}

pub(super) async fn provider_checks(
    client: &ApiClient,
    settings: &[(String, Value)],
    chosen: Option<&str>,
) -> Vec<Finding> {
    let mut providers = providers_in(settings);
    if let Some(chosen) = chosen
        && !providers.iter().any(|p| p == chosen)
    {
        providers.push(chosen.to_string());
    }
    let mut findings = Vec::new();
    for provider in providers {
        let model = first_model_of(settings, &provider);
        findings.push(check_provider(client, &provider, model).await);
    }
    findings
}

pub(super) async fn health_rows(client: &ApiClient) -> Vec<Finding> {
    let Some(payload) = client.capabilities().await else {
        return vec![Finding::failed(
            "The proxy did not return its capability rows.",
        )];
    };
    payload
        .capabilities
        .iter()
        .filter(|row| HEALTH_ROWS.contains(&row.id.as_str()))
        .map(|row| match row.state {
            CapabilityState::Active => Finding::ok(format!("{}: active", row.label)),
            CapabilityState::Off => Finding::note(format!("{}: off, no job needs it", row.label)),
            CapabilityState::Degraded | CapabilityState::Inert => Finding::failed(format!(
                "{}: {}. {} {}",
                row.label,
                if row.state == CapabilityState::Inert {
                    "inert"
                } else {
                    "degraded"
                },
                row.detail,
                row.fix
            )),
        })
        .collect()
}
