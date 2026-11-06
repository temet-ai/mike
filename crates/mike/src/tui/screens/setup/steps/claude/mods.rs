use std::path::PathBuf;

use async_trait::async_trait;
use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;

use super::common::{base_outcome, failure};
use crate::claude_code::{add_mods, find_mods_source, list_mods};
use crate::finding::Finding;
use crate::tui::screens::setup::panel::{Choice, Panel, PanelEvent};
use crate::tui::screens::setup::step::{Cx, Hints, Outcome, WizardStep};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ModsAction {
    Install,
    InstallWithWiretap,
    Leave,
}

pub(super) struct ModsInputs {
    pub(super) root: Option<PathBuf>,
    pub(super) default_set: Vec<String>,
    pub(super) with_wiretap: Vec<String>,
}

fn names(dirs: &[PathBuf]) -> Vec<String> {
    dirs.iter()
        .filter_map(|d| d.file_name().and_then(|n| n.to_str()).map(str::to_string))
        .collect()
}

pub(super) fn inputs_for(root: Option<PathBuf>) -> ModsInputs {
    match root {
        Some(root) => ModsInputs {
            default_set: names(&list_mods(&root, false)),
            with_wiretap: names(&list_mods(&root, true)),
            root: Some(root),
        },
        None => ModsInputs {
            root: None,
            default_set: Vec::new(),
            with_wiretap: Vec::new(),
        },
    }
}

pub(super) fn plan_panel(inputs: &ModsInputs) -> Panel<ModsAction> {
    let leave = Choice::new(
        ModsAction::Leave,
        "Do not install the mods",
        "Claude Code then has no mike panes or status line. `mike claude-code mods` installs them later.",
    );
    let Some(root) = &inputs.root else {
        return Panel::new(
            vec!["No folder of mods was found beside this mike binary or in a checkout".to_string()],
            vec![leave],
        )
        .with_note("A Homebrew install keeps the mods in the share/mike/mods folder under the Homebrew prefix. A mike built without them has nothing to install here.");
    };
    let mut choices = vec![
        Choice::new(
            ModsAction::Install,
            format!("Install the mods: {}", inputs.default_set.join(", ")),
            "Sets CLAUDE_CODE_PLUGIN_DIRS in the env section of ~/.claude/settings.json to these folders, keeps every other setting and every folder that is already listed, and records what it added so that `mike uninstall` removes only that.",
        )
        .recommended()
        .command("mike claude-code mods"),
    ];
    if inputs.with_wiretap.len() > inputs.default_set.len() {
        choices.push(
            Choice::new(
                ModsAction::InstallWithWiretap,
                "Install them and wiretap",
                "wiretap writes every prompt and tool call of every session to a file, so it is off unless you ask for it.",
            )
            .command("mike claude-code mods --with-wiretap"),
        );
    }
    choices.push(leave);
    Panel::new(vec![format!("Mods folder: {}", root.display())], choices)
}

pub(super) struct ModsStep {
    panel: Panel<ModsAction>,
    root: Option<PathBuf>,
}

impl ModsStep {
    pub(super) fn new() -> Self {
        Self {
            panel: Panel::default(),
            root: None,
        }
    }

    fn execute(&mut self, cx: &Cx<'_>) {
        let Some(action) = self.panel.selected().map(|c| c.action) else {
            return;
        };
        let with_wiretap = action == ModsAction::InstallWithWiretap;
        let findings = match (action, &self.root) {
            (ModsAction::Leave, _) => vec![Finding::note(
                "Nothing was changed. Run `mike claude-code mods` later to install the mods.",
            )],
            (_, Some(root)) => match add_mods(&cx.env.locations, root, with_wiretap) {
                Ok(findings) => findings,
                Err(e) => failure(e),
            },
            (_, None) => failure("No folder of mods was found."),
        };
        self.panel.record(findings);
    }
}

#[async_trait(?Send)]
impl WizardStep for ModsStep {
    fn names(&self) -> &'static [&'static str] {
        &["claude"]
    }

    fn label(&self) -> &'static str {
        "claude code"
    }

    async fn enter(&mut self, _cx: &mut Cx<'_>) {
        self.root = find_mods_source();
        self.panel = plan_panel(&inputs_for(self.root.clone()));
    }

    async fn key(&mut self, key: KeyEvent, cx: &mut Cx<'_>) -> Outcome {
        let event = self.panel.handle(key);
        if let Some(outcome) = base_outcome(event) {
            return outcome;
        }
        if event == PanelEvent::Run {
            self.execute(cx);
        } else {
            self.enter(cx).await;
        }
        Outcome::Stay
    }

    fn draw(&self, frame: &mut Frame, area: Rect) {
        self.panel.draw(frame, area, "claude code · mods");
    }

    fn hints(&self) -> Hints {
        self.panel.hints()
    }
}
