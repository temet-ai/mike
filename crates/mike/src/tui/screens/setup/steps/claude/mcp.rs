use std::path::PathBuf;

use async_trait::async_trait;
use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;

use super::common::{base_outcome, failure};
use crate::claude_code::{
    Registration, connect_mcp, inspect_registration, inspect_url_for, locate_mcp_binary,
};
use crate::db_provision::{Call, Runner, SystemRunner};
use crate::finding::Finding;
use crate::tui::screens::setup::facts::{Tool, ToolFact};
use crate::tui::screens::setup::panel::{Choice, Panel, PanelEvent};
use crate::tui::screens::setup::step::{Cx, Hints, Outcome, WizardStep};

const BREW_FORMULA: &str = "temet-ai/mike/mike-mcp";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum McpAction {
    Register,
    InstallWithBrew,
    Leave,
}

pub(super) struct McpInputs {
    pub(super) claude: Option<ToolFact>,
    pub(super) binary: Option<PathBuf>,
    pub(super) registration: Result<Registration, String>,
    pub(super) brew: bool,
    pub(super) inspect_url: String,
}

fn found_lines(inputs: &McpInputs) -> Vec<String> {
    let mut lines = vec![match &inputs.claude {
        Some(fact) => fact.describe(Tool::Claude),
        None => "Claude Code (`claude`) is not on PATH".to_string(),
    }];
    lines.push(match &inputs.binary {
        Some(path) => format!("mike-mcp: {}", path.display()),
        None => "mike-mcp was not found beside `mike` or on PATH".to_string(),
    });
    lines.push(match &inputs.registration {
        Ok(r) if r.user_scope() => format!(
            "~/.claude.json registers mike with {}{}",
            r.command.as_deref().unwrap_or_default(),
            if r.command_exists {
                ""
            } else {
                " (that file does not exist)"
            }
        ),
        Ok(r) if r.in_a_project => "~/.claude.json registers mike for one project only".to_string(),
        Ok(_) => "~/.claude.json does not register mike yet".to_string(),
        Err(why) => why.clone(),
    });
    lines
}

fn register_choice(inputs: &McpInputs) -> Choice<McpAction> {
    Choice::new(
        McpAction::Register,
        "Register mike-mcp for Claude Code",
        format!(
            "Adds a `mike` entry under mcpServers in ~/.claude.json with the absolute path of mike-mcp and MIKE_INSPECT_URL={}, keeps every other key in that file, then starts mike-mcp once and counts the tools it lists.",
            inputs.inspect_url
        ),
    )
    .recommended()
    .command("mike claude-code mcp")
}

pub(super) fn plan_panel(inputs: &McpInputs) -> Panel<McpAction> {
    let mut choices = Vec::new();
    if inputs.binary.is_some() {
        choices.push(register_choice(inputs));
    } else if inputs.brew {
        choices.push(
            Choice::new(
                McpAction::InstallWithBrew,
                "Install mike-mcp with Homebrew",
                "mike-mcp is a separate program from `mike`; this installs it, and registration follows.",
            )
            .recommended()
            .command(format!("brew install {BREW_FORMULA}")),
        );
    }
    choices.push(Choice::new(
        McpAction::Leave,
        "Do not register mike-mcp",
        "Claude Code then has no mike tools. `mike claude-code mcp` registers them later.",
    ));
    Panel::new(found_lines(inputs), choices)
}

pub(super) struct Mcp {
    panel: Panel<McpAction>,
}

impl Mcp {
    pub(super) fn new() -> Self {
        Self {
            panel: Panel::default(),
        }
    }

    async fn inputs(cx: &mut Cx<'_>) -> McpInputs {
        let path_var = cx.env.locations.path_var.clone();
        cx.env
            .facts
            .ensure(
                &SystemRunner,
                path_var.as_deref(),
                &[Tool::Claude, Tool::Brew],
            )
            .await;
        McpInputs {
            claude: cx.env.facts.get(Tool::Claude).cloned(),
            binary: locate_mcp_binary(&cx.env.locations),
            registration: inspect_registration(&cx.env.locations).map_err(|e| e.to_string()),
            brew: cx.env.facts.has(Tool::Brew),
            inspect_url: inspect_url_for(cx.config),
        }
    }

    async fn execute(&mut self, cx: &mut Cx<'_>) {
        let Some(action) = self.panel.selected().map(|c| c.action) else {
            return;
        };
        match action {
            McpAction::Register => {
                let url = inspect_url_for(cx.config);
                let findings = match connect_mcp(&cx.env.locations, &url, true).await {
                    Ok(findings) => findings,
                    Err(e) => failure(e),
                };
                self.panel.record(findings);
            }
            McpAction::InstallWithBrew => self.install(cx).await,
            McpAction::Leave => self.panel.record(vec![Finding::note(
                "Nothing was changed. Run `mike claude-code mcp` later to register mike-mcp.",
            )]),
        }
    }

    async fn install(&mut self, cx: &mut Cx<'_>) {
        let call = Call::new("brew", &["install", BREW_FORMULA]);
        let findings = match SystemRunner.run(&call).await {
            Ok(out) if out.success => {
                self.enter(cx).await;
                self.panel.show(vec![Finding::ok(format!(
                    "`{}` finished. Registration is the next choice.",
                    call.shown()
                ))]);
                return;
            }
            Ok(out) => failure(format!("`{}` failed: {}", call.shown(), out.detail())),
            Err(e) => failure(format!("`{}` could not run: {e}", call.shown())),
        };
        self.panel.record(findings);
    }
}

#[async_trait(?Send)]
impl WizardStep for Mcp {
    fn names(&self) -> &'static [&'static str] {
        &["claude"]
    }

    fn label(&self) -> &'static str {
        "claude code"
    }

    async fn enter(&mut self, cx: &mut Cx<'_>) {
        let inputs = Self::inputs(cx).await;
        self.panel = plan_panel(&inputs);
    }

    async fn key(&mut self, key: KeyEvent, cx: &mut Cx<'_>) -> Outcome {
        let event = self.panel.handle(key);
        if let Some(outcome) = base_outcome(event) {
            return outcome;
        }
        if event == PanelEvent::Run {
            self.execute(cx).await;
        } else {
            self.enter(cx).await;
        }
        Outcome::Stay
    }

    fn draw(&self, frame: &mut Frame, area: Rect) {
        self.panel.draw(frame, area, "claude code · mike tools");
    }

    fn hints(&self) -> Hints {
        self.panel.hints()
    }
}
