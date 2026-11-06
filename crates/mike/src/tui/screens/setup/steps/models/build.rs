use serde_json::{Value, json};

use super::form::{Cell, Form, Role, Row};
use super::jobs::{Job, Tier, used_by};
use super::source::{Source, normalize, provider_of};

pub(super) const DAILY_KEY: &str = "llm.daily_spend_ceiling_usd";
pub(super) const CALLS_KEY: &str = "llm.ceiling_max_calls";
pub(super) const RATE_KEY: &str = "llm.rate_limit_per_minute";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Mode {
    One(String),
    PerJob,
}

fn row(role: Role, label: &str, detail: String, cells: Vec<Cell>) -> Row {
    Row {
        role,
        label: label.to_string(),
        detail,
        cells,
        focus: 0,
    }
}

fn provider_ids(source: &Source) -> Vec<String> {
    source.available().iter().map(|p| p.id.clone()).collect()
}

fn tier_row(source: &Source, provider: &str, tier: Tier) -> Row {
    let current = normalize(provider, &source.tier_default(provider, tier));
    row(
        Role::Tier(tier),
        &format!("Model for the {} tier", tier.name()),
        format!("Used by: {}", used_by(&source.running, tier)),
        vec![Cell::pick(source.model_options(provider), &current, true)],
    )
}

fn job_row(source: &Source, index: usize, job: &Job) -> Row {
    let stored = source.text(job.key);
    let stored_provider = provider_of(&stored);
    let provider =
        if source.available().iter().any(|p| p.id == stored_provider) && !stored.is_empty() {
            stored_provider.to_string()
        } else {
            source.first_available().unwrap_or_default()
        };
    let model = if !stored.is_empty() && provider_of(&stored) == provider {
        normalize(&provider, &stored)
    } else {
        normalize(&provider, &source.tier_default(&provider, job.tier))
    };
    row(
        Role::Job(index),
        job.name,
        format!("{} tier", job.tier.name()),
        vec![
            Cell::pick(provider_ids(source), &provider, false),
            Cell::pick(source.model_options(&provider), &model, true),
        ],
    )
}

fn limit_rows(source: &Source) -> Vec<Row> {
    let value = |key: &str, fallback: &str| {
        let text = source.text(key);
        if text.is_empty() {
            fallback.to_string()
        } else {
            text
        }
    };
    vec![
        row(
            Role::DailySpend,
            "Daily spend limit in US dollars",
            "0 means no daily limit".to_string(),
            vec![Cell::text(&value(DAILY_KEY, "0"))],
        ),
        row(
            Role::MaxCalls,
            "Most calls in the rolling window",
            "the window is one hour unless llm.ceiling_window_secs says otherwise".to_string(),
            vec![Cell::text(&value(CALLS_KEY, "100"))],
        ),
        row(
            Role::RatePerMinute,
            "Most calls in one minute",
            String::new(),
            vec![Cell::text(&value(RATE_KEY, "12"))],
        ),
    ]
}

fn submit_row() -> Row {
    row(
        Role::Submit,
        "Save these and check them",
        String::new(),
        Vec::new(),
    )
}

pub(super) fn build_form(mode: &Mode, source: &Source) -> Form {
    let mut rows = Vec::new();
    match mode {
        Mode::One(provider) => {
            rows.push(row(
                Role::Provider,
                "Provider for every job",
                String::new(),
                vec![Cell::pick(provider_ids(source), provider, false)],
            ));
            rows.extend(Tier::ALL.map(|tier| tier_row(source, provider, tier)));
        }
        Mode::PerJob => {
            rows.extend(
                source
                    .running
                    .iter()
                    .enumerate()
                    .map(|(index, job)| job_row(source, index, job)),
            );
        }
    }
    rows.extend(limit_rows(source));
    rows.push(submit_row());
    let mut note = String::from(
        "Claude and Codex calls use your own Claude or ChatGPT plan, so the spend limits below count OpenRouter calls only.",
    );
    if source.turn_guard && matches!(mode, Mode::PerJob) {
        note.push_str(" The turn guard has no model setting of its own and uses the strong model of the provider chosen for every job; choose one provider for every job if you want to set it.");
    }
    Form {
        rows,
        cursor: 0,
        note: Some(note),
        ..Form::default()
    }
}

pub(super) fn refresh_after_change(form: &mut Form, source: &Source, changed: usize, cell: usize) {
    let Some(role) = form.rows.get(changed).map(|r| r.role) else {
        return;
    };
    match role {
        Role::Provider => {
            let provider = form.rows[changed].cells[0].value();
            for tier in Tier::ALL {
                if let Some(index) = form.rows.iter().position(|r| r.role == Role::Tier(tier)) {
                    form.rows[index] = tier_row(source, &provider, tier);
                }
            }
        }
        Role::Job(index) if cell == 0 => {
            let provider = form.rows[changed].cells[0].value();
            if let Some(job) = source.running.get(index) {
                let model = normalize(&provider, &source.tier_default(&provider, job.tier));
                form.rows[changed].cells[1] =
                    Cell::pick(source.model_options(&provider), &model, true);
            }
        }
        _ => {}
    }
}

fn limit_value(form: &Form, role: Role) -> String {
    form.rows
        .iter()
        .find(|r| r.role == role)
        .and_then(|r| r.cells.first())
        .map(|c| c.value().trim().to_string())
        .unwrap_or_default()
}

fn limits(form: &Form) -> Result<Vec<(String, Value)>, String> {
    let daily: f64 = limit_value(form, Role::DailySpend)
        .parse()
        .ok()
        .filter(|v: &f64| (0.0..=1000.0).contains(v))
        .ok_or("The daily spend limit must be a number from 0 to 1000.")?;
    let calls: i64 = limit_value(form, Role::MaxCalls)
        .parse()
        .ok()
        .filter(|v| (1..=100_000).contains(v))
        .ok_or("The call limit must be a whole number from 1 to 100000.")?;
    let rate: i64 = limit_value(form, Role::RatePerMinute)
        .parse()
        .ok()
        .filter(|v| (1..=600).contains(v))
        .ok_or("The calls-per-minute limit must be a whole number from 1 to 600.")?;
    Ok(vec![
        (DAILY_KEY.to_string(), json!(daily)),
        (CALLS_KEY.to_string(), json!(calls)),
        (RATE_KEY.to_string(), json!(rate)),
    ])
}

fn model_value(provider: &str, cell: &Cell) -> Result<String, String> {
    let text = cell.value();
    if text.trim().is_empty() {
        return Err("A model is blank.".to_string());
    }
    let trimmed = text.trim();
    let wrong = (provider != "claude" && trimmed.starts_with("claude/"))
        || (provider != "codex" && trimmed.starts_with("codex/"))
        || (provider == "claude" && trimmed.starts_with("codex/"))
        || (provider == "codex" && trimmed.starts_with("claude/"));
    if wrong {
        return Err(format!(
            "{trimmed} is not a {provider} model; pick the provider that matches it."
        ));
    }
    Ok(normalize(provider, trimmed))
}

pub(super) fn collect(
    form: &Form,
    source: &Source,
    mode: &Mode,
) -> Result<Vec<(String, Value)>, String> {
    let mut settings = Vec::new();
    match mode {
        Mode::One(_) => {
            let provider = form
                .rows
                .iter()
                .find(|r| r.role == Role::Provider)
                .map(|r| r.cells[0].value())
                .unwrap_or_default();
            settings.push(("llm.provider".to_string(), json!(provider)));
            for tier in Tier::ALL {
                let found = form.rows.iter().find(|r| r.role == Role::Tier(tier));
                if let Some(found) = found {
                    let model = model_value(&provider, &found.cells[0])?;
                    settings.push((
                        format!("llm.provider_models.{provider}.{}", tier.name()),
                        json!(model),
                    ));
                }
            }
        }
        Mode::PerJob => {
            settings.push(("llm.provider".to_string(), json!("per_role")));
            for found in &form.rows {
                let Role::Job(index) = found.role else {
                    continue;
                };
                let job = source.running.get(index).ok_or("A job row has no job.")?;
                let provider = found.cells[0].value();
                let model = model_value(&provider, &found.cells[1])?;
                settings.push((job.key.to_string(), json!(model)));
            }
            same_model_check(&settings)?;
        }
    }
    settings.extend(limits(form)?);
    Ok(settings)
}

fn same_model_check(settings: &[(String, Value)]) -> Result<(), String> {
    let get = |key: &str| {
        settings
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    };
    let proposer = get("lesson_relations.proposer_model");
    let adjudicator = get("lesson_relations.adjudicator_model");
    if proposer.is_some() && proposer == adjudicator {
        return Err(
            "The lesson-relations proposer and adjudicator must use different models.".to_string(),
        );
    }
    Ok(())
}

pub(super) fn providers_in(settings: &[(String, Value)]) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for (key, value) in settings {
        if key == "llm.provider" {
            continue;
        }
        if let Some(text) = value.as_str() {
            let provider = provider_of(text).to_string();
            if (key.starts_with("llm.provider_models.")
                || key.ends_with("model")
                || key.ends_with("_model"))
                && !found.contains(&provider)
            {
                found.push(provider);
            }
        }
    }
    found
}
