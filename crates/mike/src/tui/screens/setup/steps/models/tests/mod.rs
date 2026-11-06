use std::collections::HashMap;

use crossterm::event::KeyCode;
use serde_json::{Value, json};
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::Models;
use super::build::{Mode, build_form, collect, providers_in, refresh_after_change};
use super::check::{health_rows, read_back, send_settings};
use super::form::{Cell, FormEvent, Role};
use super::intro::{IntroAction, idle_panel, plan_panel};
use super::jobs::{JOBS, Tier, running, used_by};
use super::source::{Source, normalize, provider_of};
use crate::finding::all_passed;
use crate::tui::client::{ApiClient, ProviderDto, ProviderModelDto, ProvidersPayload};
use crate::tui::screens::setup::harness::{Harness, key, render_step, typed};
use crate::tui::screens::setup::step::{Outcome, WizardStep};

fn model(provider: &str, id: &str, good: bool) -> ProviderModelDto {
    let model_ref = match provider {
        "openrouter" => format!("openrouter/{id}"),
        other => format!("{other}/{id}"),
    };
    ProviderModelDto {
        id: id.to_string(),
        model_ref,
        label: id.to_string(),
        known_good: Some(good),
    }
}

fn provider(id: &str, available: bool, models: Vec<ProviderModelDto>) -> ProviderDto {
    ProviderDto {
        id: id.to_string(),
        label: match id {
            "openrouter" => "OpenRouter".to_string(),
            "claude" => "Claude".to_string(),
            _ => "Codex".to_string(),
        },
        prefix: format!("{id}/"),
        available,
        detail: format!("{id} is ready"),
        fix: format!("fix {id}"),
        key_env_var: (id == "openrouter").then(|| "MIKE_STEERER_OPENROUTER_API_KEY".to_string()),
        models,
        models_error: None,
    }
}

fn payload(openrouter_available: bool) -> ProvidersPayload {
    ProvidersPayload {
        llm_provider: "per_role".to_string(),
        providers: vec![
            provider(
                "openrouter",
                openrouter_available,
                vec![
                    model("openrouter", "anthropic/claude-sonnet-5-5", true),
                    model("openrouter", "google/gemini-3.8-flash", false),
                    model("openrouter", "openai/gpt-6-sol", true),
                ],
            ),
            provider(
                "claude",
                true,
                vec![
                    model("claude", "haiku", false),
                    model("claude", "sonnet", false),
                    model("claude", "opus", false),
                ],
            ),
            provider("codex", false, vec![model("codex", "gpt-6-sol", false)]),
        ],
    }
}

fn values() -> HashMap<String, Value> {
    HashMap::from([
        ("llm.provider".to_string(), json!("per_role")),
        (
            "llm.provider_models.claude.fast".to_string(),
            json!("claude/haiku"),
        ),
        (
            "llm.provider_models.claude.standard".to_string(),
            json!("claude/sonnet"),
        ),
        (
            "llm.provider_models.claude.strong".to_string(),
            json!("claude/opus"),
        ),
        ("llm.daily_spend_ceiling_usd".to_string(), json!(0.0)),
        ("llm.ceiling_max_calls".to_string(), json!(100)),
        ("llm.rate_limit_per_minute".to_string(), json!(12)),
        (
            "judge_worker.model".to_string(),
            json!("google/gemini-3.8-flash"),
        ),
        ("session_judge.model".to_string(), json!("claude/haiku")),
        ("steerer.model".to_string(), json!("")),
    ])
}

fn on(keys: &[&str]) -> Vec<(String, bool)> {
    keys.iter().map(|k| ((*k).to_string(), true)).collect()
}

fn source(openrouter_available: bool, switches: &[&str], relations: bool) -> Source {
    Source::new(
        payload(openrouter_available),
        values(),
        &on(switches),
        relations,
        false,
    )
}

mod build_tests;
mod check_tests;
