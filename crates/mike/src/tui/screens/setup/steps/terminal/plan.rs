use crate::launch::{BackendId, TerminalBackend, backend_for};
use crate::tui::screens::setup::facts::{Tool, ToolFact};
use crate::tui::screens::setup::panel::{Choice, Panel};

pub(super) type BackendSource = fn(BackendId) -> Option<Box<dyn TerminalBackend>>;

pub(super) const HERDR_SITE: &str = "https://herdr.dev";

pub(super) fn real_source() -> BackendSource {
    if crate::sandbox::active() {
        sandbox_backend_for
    } else {
        backend_for
    }
}

pub(super) fn sandbox_backend_for(id: BackendId) -> Option<Box<dyn TerminalBackend>> {
    (id == BackendId::None).then(|| backend_for(id)).flatten()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TermAction {
    Backend(BackendId),
    InstallHerdr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Probed {
    pub(super) id: BackendId,
    pub(super) available: bool,
    pub(super) remedy: String,
}

#[derive(Debug, Clone)]
pub(super) struct TermInputs {
    pub(super) herdr: Option<ToolFact>,
    pub(super) tmux: Option<ToolFact>,
    pub(super) hyprctl: bool,
    pub(super) xdg: bool,
    pub(super) brew: bool,
    pub(super) current: Option<BackendId>,
    pub(super) probed: Vec<Probed>,
}

pub(super) fn trade_off(id: BackendId) -> &'static str {
    match id {
        BackendId::Herdr => {
            "A persistent server with agent states and a session-uuid join; the fleet's first backend."
        }
        BackendId::HerdrCore => {
            "Tabs in the Herdr desktop app through herdr-core (`hc`), with agent states."
        }
        BackendId::Tmux => {
            "For headless hosts, ssh and containers: no display needed, no agent state."
        }
        BackendId::Terminal => {
            "Plain windows through xdg-terminal-exec and hyprctl: window placement only, no agent state."
        }
        BackendId::Ghostty => "Ghostty windows on macOS through AppleScript.",
        BackendId::None => {
            "Launch by hand: `mike claude` works, `mike spawn` refuses and the fleet protocol is unavailable."
        }
    }
}

pub(super) fn installed(inputs: &TermInputs) -> Vec<BackendId> {
    let mut list = Vec::new();
    if inputs.herdr.is_some() {
        list.push(BackendId::Herdr);
    }
    if inputs.tmux.is_some() {
        list.push(BackendId::Tmux);
    }
    if inputs.hyprctl && inputs.xdg {
        list.push(BackendId::Terminal);
    }
    list.push(BackendId::None);
    list
}

pub(super) fn ordered(inputs: &TermInputs) -> Vec<BackendId> {
    let mut list = installed(inputs);
    if let Some(current) = inputs.current
        && let Some(at) = list.iter().position(|id| *id == current)
    {
        let id = list.remove(at);
        list.insert(0, id);
    }
    list
}

fn found_lines(inputs: &TermInputs) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(fact) = &inputs.herdr {
        lines.push(fact.describe(Tool::Herdr));
    }
    if let Some(fact) = &inputs.tmux {
        lines.push(fact.describe(Tool::Tmux));
    }
    if inputs.hyprctl && inputs.xdg {
        lines.push("hyprctl and xdg-terminal-exec".to_string());
    }
    for probed in &inputs.probed {
        lines.push(if probed.available {
            format!("{} is ready", probed.id)
        } else if probed.remedy.is_empty() {
            format!("{} is not ready", probed.id)
        } else {
            format!("{} is not ready: {}", probed.id, probed.remedy)
        });
    }
    if lines.is_empty() {
        lines.push("No terminal tool was found: herdr, tmux and hyprctl with xdg-terminal-exec are all absent".to_string());
    }
    lines
}

fn install_herdr_choice(inputs: &TermInputs) -> Choice<TermAction> {
    let choice = Choice::new(
        TermAction::InstallHerdr,
        "Install herdr",
        format!(
            "herdr is the backend the fleet protocol is built on: a persistent server with agent states. Its home page is {HERDR_SITE}."
        ),
    )
    .recommended();
    if inputs.brew {
        choice.command("brew install herdr")
    } else {
        choice
    }
}

fn backend_choice(inputs: &TermInputs, id: BackendId) -> Choice<TermAction> {
    let current = if inputs.current == Some(id) {
        " (current)"
    } else {
        ""
    };
    let check = if id == BackendId::None {
        "Saved without a live check."
    } else {
        "Checks it live by opening and closing a scratch workspace, and saves it only when that passes."
    };
    let choice = Choice::new(
        TermAction::Backend(id),
        format!("{id}{current}"),
        format!("{} {check}", trade_off(id)),
    )
    .command(format!("MIKE_LAUNCH_BACKEND={id} mike spawn doctor"));
    if id == BackendId::Herdr {
        choice.recommended()
    } else {
        choice
    }
}

pub(super) fn plan_panel(inputs: &TermInputs) -> Panel<TermAction> {
    let mut choices = Vec::new();
    if inputs.herdr.is_none() {
        choices.push(install_herdr_choice(inputs));
    }
    choices.extend(
        ordered(inputs)
            .into_iter()
            .map(|id| backend_choice(inputs, id)),
    );
    Panel::new(found_lines(inputs), choices)
}
