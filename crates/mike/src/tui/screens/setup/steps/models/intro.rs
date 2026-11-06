use super::source::Source;
use crate::tui::screens::setup::panel::{Choice, Input, Panel};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum IntroAction {
    Provider(String),
    PerJob,
    SetUpOpenRouter,
    Leave,
}

fn provider_why(id: &str) -> &'static str {
    match id {
        "claude" => {
            "Runs each call with `claude -p` under the Claude Code login on this machine, so it needs no key and costs no OpenRouter spend; it uses your own Claude plan."
        }
        "codex" => {
            "Runs each call with `codex exec` under the ChatGPT login on this machine, so it needs no key and costs no OpenRouter spend; it uses your own plan."
        }
        _ => {
            "Calls OpenRouter with your key, which also reaches Claude and GPT models. The spend limits on the next screen apply to these calls."
        }
    }
}

fn found_lines(source: &Source) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for provider in source.available() {
        lines.push(format!("{}: {}", provider.label, provider.detail));
    }
    for provider in source.unavailable() {
        lines.push(format!(
            "{}: not available. {}",
            provider.label, provider.fix
        ));
    }
    let names: Vec<&str> = source.running.iter().map(|job| job.name).collect();
    lines.push(format!("Jobs that need a model: {}", names.join(", ")));
    lines
}

fn openrouter_setup_choice(source: &Source, default: &str) -> Choice<IntroAction> {
    let mut why = format!(
        "Type the name of an environment variable that holds your key and only the name is saved, or paste the key and it is saved in the owner-only config file. The running proxy reads the key from {}.",
        source.key_variable
    );
    if source.service_installed {
        why.push_str(" The background service starts without your shell's variables, so a variable name only helps if the service environment sets it; pasting the key always works.");
    }
    Choice::new(
        IntroAction::SetUpOpenRouter,
        "Set up an OpenRouter key",
        why,
    )
    .input(
        Input::new("variable name or key", "OPENROUTER_API_KEY")
            .secret()
            .with_value(default),
    )
}

pub(super) fn plan_panel(source: &Source, openrouter_default: &str) -> Panel<IntroAction> {
    let mut choices = Vec::new();
    for (index, provider) in source.available().into_iter().enumerate() {
        let choice = Choice::new(
            IntroAction::Provider(provider.id.clone()),
            format!("Use {} for every job", provider.label),
            provider_why(&provider.id),
        );
        choices.push(if index == 0 {
            choice.recommended()
        } else {
            choice
        });
    }
    if !source.available().is_empty() {
        choices.push(Choice::new(
            IntroAction::PerJob,
            "Choose a provider and a model for each job",
            "One row per job, each with its own provider and a model from that provider's list.",
        ));
    }
    if source.provider("openrouter").is_some_and(|p| !p.available) {
        choices.push(openrouter_setup_choice(source, openrouter_default));
    }
    choices.push(Choice::new(
        IntroAction::Leave,
        "Leave the models as they are",
        "Every job keeps the model its own setting names.",
    ));
    Panel::new(found_lines(source), choices)
}

pub(super) fn idle_panel(note: &str) -> Panel<IntroAction> {
    Panel::default().with_note(note)
}
