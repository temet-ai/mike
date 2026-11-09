use std::path::PathBuf;

use super::check::MODEL_NAME;
use super::install::{LLAMA_VERSION, SOCKET_UNIT};
use crate::tui::screens::setup::panel::{Choice, Input, Panel};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RerankAction {
    InstallHere,
    OtherMachine,
    Leave,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Current {
    pub(super) enabled: bool,
    pub(super) url: String,
}

#[derive(Debug, Clone, Default)]
pub(super) struct RerankInputs {
    pub(super) gpu: Option<String>,
    pub(super) systemd: bool,
    pub(super) proxyd: Option<PathBuf>,
    pub(super) server_installed: bool,
    pub(super) model_installed: bool,
    pub(super) socket_active: bool,
    pub(super) current: Option<Current>,
}

impl RerankInputs {
    pub(super) fn can_host(&self) -> bool {
        self.gpu.is_some() && self.systemd && self.proxyd.is_some()
    }
}

fn host_lines(inputs: &RerankInputs) -> Vec<String> {
    let mut lines = vec![match &inputs.gpu {
        Some(gpu) => format!("Graphics card: {gpu}"),
        None => "No NVIDIA graphics card answers `nvidia-smi -L`, so the reranker would be too slow here".to_string(),
    }];
    if !inputs.systemd {
        lines.push(
            "No systemd user manager answers here, so this machine cannot start the reranker on demand"
                .to_string(),
        );
    } else if inputs.proxyd.is_none() {
        lines.push("systemd-socket-proxyd is not installed".to_string());
    }
    lines
}

fn install_lines(inputs: &RerankInputs) -> Vec<String> {
    let installed = |done: bool, what: String| {
        if done {
            format!("{what} is installed")
        } else {
            format!("{what} is not installed yet")
        }
    };
    vec![
        installed(
            inputs.server_installed,
            format!("llama-server {LLAMA_VERSION}"),
        ),
        installed(inputs.model_installed, format!("The model {MODEL_NAME}")),
        if inputs.socket_active {
            format!("{SOCKET_UNIT} is listening on port 11435")
        } else {
            format!("{SOCKET_UNIT} is not listening")
        },
    ]
}

fn found_lines(inputs: &RerankInputs) -> Vec<String> {
    let mut lines = host_lines(inputs);
    if inputs.can_host() {
        lines.extend(install_lines(inputs));
    }
    lines.push(match &inputs.current {
        Some(c) if c.enabled => format!("Reranking is on: {}", c.url),
        Some(c) if c.url.is_empty() => "Reranking is off".to_string(),
        Some(c) => format!("Reranking is off (address {})", c.url),
        None => "The proxy did not answer, so its current setting is unknown".to_string(),
    });
    lines
}

fn install_choice(inputs: &RerankInputs) -> Choice<RerankAction> {
    let mut choice = Choice::new(
        RerankAction::InstallHere,
        "Run the reranker on this machine",
        format!(
            "Downloads llama-server {LLAMA_VERSION} (about 35 MB) and {MODEL_NAME} (about 640 MB) into ~/.local/share/mike, skipping what is there, writes three systemd user units, and starts listening on port 11435 on every interface so other machines can use it too. The server loads on the first search and unloads after 10 idle minutes, using about 1 GB of graphics memory while loaded. Then it reranks three passages to check, and sets rerank.url, rerank.model and rerank.enabled in the running proxy."
        ),
    )
    .recommended();
    if !inputs.socket_active {
        choice = choice.command(format!("systemctl --user enable --now {SOCKET_UNIT}"));
    }
    choice
}

pub(super) fn plan_panel(inputs: &RerankInputs) -> Panel<RerankAction> {
    let mut choices = Vec::new();
    if inputs.can_host() {
        choices.push(install_choice(inputs));
    }
    let mut other = Choice::new(
        RerankAction::OtherMachine,
        "Use the reranker on another machine",
        "Any llama-server started with --reranking, such as one this step set up on a machine with a graphics card. Reranks three passages there to check, then sets rerank.url, rerank.model and rerank.enabled. Search queries and passages are sent to that machine.",
    )
    .input(
        Input::new("address", "http://omit:11435")
            .with_value(inputs.current.as_ref().map_or("", |c| c.url.as_str())),
    );
    if !inputs.can_host() {
        other = other.recommended();
    }
    choices.push(other);
    choices.push(Choice::new(
        RerankAction::Leave,
        "Leave reranking off",
        "Searches keep the order search by meaning gives them.",
    ));
    Panel::new(found_lines(inputs), choices)
}
