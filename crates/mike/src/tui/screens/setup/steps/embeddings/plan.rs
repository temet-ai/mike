use super::check::{OLLAMA_MODEL, has_model};
use crate::tui::screens::setup::facts::{Tool, ToolFact};
use crate::tui::screens::setup::panel::{Choice, Input, Panel};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EmbedAction {
    UseOllama,
    InstallWithBrew,
    PrintInstall,
    OtherServer,
    Leave,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Current {
    pub(super) enabled: bool,
    pub(super) url: String,
    pub(super) model: String,
}

#[derive(Debug, Clone)]
pub(super) struct EmbedInputs {
    pub(super) base: String,
    pub(super) ollama: Option<ToolFact>,
    pub(super) models: Option<Vec<String>>,
    pub(super) brew: bool,
    pub(super) package_manager: Option<Tool>,
    pub(super) current: Option<Current>,
}

fn found_lines(inputs: &EmbedInputs) -> Vec<String> {
    let mut lines = vec![match &inputs.ollama {
        Some(fact) => fact.describe(Tool::Ollama),
        None => "Ollama is not installed".to_string(),
    }];
    if inputs.ollama.is_some() {
        lines.push(match &inputs.models {
            Some(models) if has_model(models, OLLAMA_MODEL) => {
                format!(
                    "The Ollama server answers on {} and has {OLLAMA_MODEL}",
                    inputs.base
                )
            }
            Some(_) => format!(
                "The Ollama server answers on {}; {OLLAMA_MODEL} is not pulled yet",
                inputs.base
            ),
            None => format!("The Ollama server does not answer on {}", inputs.base),
        });
    }
    lines.push(match &inputs.current {
        Some(c) if c.enabled => format!(
            "Search by meaning is on: {} with the model {}",
            c.url, c.model
        ),
        Some(c) => format!(
            "Search by meaning is off (address {}, model {})",
            c.url, c.model
        ),
        None => "The proxy did not answer, so its current setting is unknown".to_string(),
    });
    lines
}

fn use_ollama_choice(inputs: &EmbedInputs) -> Choice<EmbedAction> {
    let pulled = inputs
        .models
        .as_ref()
        .is_some_and(|m| has_model(m, OLLAMA_MODEL));
    let mut choice = Choice::new(
        EmbedAction::UseOllama,
        format!("Use Ollama with {OLLAMA_MODEL}"),
        format!(
            "Pulls {OLLAMA_MODEL} (about 620 MB, skipped when it is already there), then sets embedding.url to {}/v1, embedding.model to {OLLAMA_MODEL} and embedding.enabled to true in the running proxy, and fetches one embedding to check it. The text being embedded stays on this machine.",
            inputs.base
        ),
    )
    .recommended();
    if !pulled {
        choice = choice.command(format!("ollama pull {OLLAMA_MODEL}"));
    }
    if inputs.models.is_none() && inputs.brew {
        choice = choice.command("brew services start ollama");
    }
    choice
}

fn install_choice(inputs: &EmbedInputs) -> Choice<EmbedAction> {
    if inputs.brew {
        return Choice::new(
            EmbedAction::InstallWithBrew,
            "Install Ollama with Homebrew",
            "Installs Ollama and starts its server as a Homebrew service, which needs no root. Setup then pulls the model.",
        )
        .recommended()
        .command("brew install ollama")
        .command("brew services start ollama");
    }
    let install = match inputs.package_manager {
        Some(Tool::Pacman) => "sudo pacman -S ollama",
        _ => "curl -fsSL https://ollama.com/install.sh | sh",
    };
    Choice::new(
        EmbedAction::PrintInstall,
        "Install Ollama",
        "mike prints the install command for this machine and does not run it, because it needs root. Run it, then run this step again.",
    )
    .recommended()
    .root_command(install)
}

pub(super) fn plan_panel(inputs: &EmbedInputs) -> Panel<EmbedAction> {
    let mut choices = Vec::new();
    if inputs.ollama.is_some() {
        choices.push(use_ollama_choice(inputs));
    } else {
        choices.push(install_choice(inputs));
    }
    choices.push(
        Choice::new(
            EmbedAction::OtherServer,
            "Use another embedding server",
            "Any server with an OpenAI-shaped /embeddings route that returns 768 numbers per text. Sets embedding.url and switches embeddings on, keeping the model name that is set now, and fetches one embedding to check it.",
        )
        .input(
            Input::new("address", "http://127.0.0.1:7997")
                .with_value(inputs.current.as_ref().map_or("", |c| c.url.as_str())),
        ),
    );
    let leave = Choice::new(
        EmbedAction::Leave,
        "Leave search by meaning off",
        "Recall uses text matching instead.",
    );
    choices.push(leave);
    Panel::new(found_lines(inputs), choices)
}
