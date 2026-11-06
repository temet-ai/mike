use async_trait::async_trait;
use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;

use super::common::{base_outcome, failure};
use crate::claude_code::{
    FUNCTION_KEY, Function, Route, Shell, ShellRequest, add_function, mike_command, read_setting,
    render_block, stored_route, suggest_function_name,
};
use crate::tui::screens::setup::panel::{Choice, Input, Panel, PanelEvent};
use crate::tui::screens::setup::step::{Cx, Hints, Outcome, WizardStep};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ShellAction {
    Add(Vec<Shell>),
    Leave,
}

pub(super) struct ShellInputs {
    pub(super) installed: Vec<(Shell, std::path::PathBuf, bool)>,
    pub(super) login: Option<Shell>,
    pub(super) route: Route,
    pub(super) name: String,
    pub(super) proxy_addr: String,
    pub(super) mike: String,
}

fn found_lines(inputs: &ShellInputs) -> Vec<String> {
    if inputs.installed.is_empty() {
        return vec!["None of zsh, bash or fish was found".to_string()];
    }
    inputs
        .installed
        .iter()
        .map(|(shell, rc, present)| {
            let state = if *present {
                "has a mike block"
            } else {
                "has no mike block"
            };
            let login = if inputs.login == Some(*shell) {
                ", your login shell"
            } else {
                ""
            };
            format!("{}{login}: {} {state}", shell.name(), rc.display())
        })
        .collect()
}

fn preview(inputs: &ShellInputs, shell: Shell) -> String {
    let function = Function {
        name: &inputs.name,
        route: inputs.route,
        proxy_addr: &inputs.proxy_addr,
        mike: &inputs.mike,
    };
    render_block(shell, &function)
}

fn add_choice(inputs: &ShellInputs, shells: Vec<Shell>, label: String) -> Choice<ShellAction> {
    let first = shells.first().copied().unwrap_or(Shell::Zsh);
    let names: Vec<&str> = shells.iter().map(|s| s.name()).collect();
    let why = format!(
        "Writes this block into the startup file of {} and replaces it in place when you run this again. Calling the function starts Claude Code through mike with route {}.\n{}",
        names.join(", "),
        inputs.route.as_str(),
        preview(inputs, first)
    );
    Choice::new(ShellAction::Add(shells), label, why)
        .input(Input::new("function name", "cc").with_value(inputs.name.clone()))
        .command(format!(
            "mike claude-code shell --shell {} --name {}",
            names.join(","),
            inputs.name
        ))
}

pub(super) fn plan_panel(inputs: &ShellInputs) -> Panel<ShellAction> {
    let mut choices = Vec::new();
    let login = inputs
        .login
        .filter(|l| inputs.installed.iter().any(|(s, _, _)| s == l));
    if let Some(login) = login {
        let label = format!("Add the function to {}, your login shell", login.name());
        choices.push(add_choice(inputs, vec![login], label).recommended());
    }
    if inputs.installed.len() > 1 || (login.is_none() && !inputs.installed.is_empty()) {
        let all: Vec<Shell> = inputs.installed.iter().map(|(s, _, _)| *s).collect();
        let label = format!(
            "Add the function to {}",
            all.iter().map(|s| s.name()).collect::<Vec<_>>().join(", ")
        );
        let mut choice = add_choice(inputs, all, label);
        choice.recommended = login.is_none();
        choices.push(choice);
    }
    choices.push(Choice::new(
        ShellAction::Leave,
        "Do not add a function",
        "Start Claude Code with `mike claude` instead.",
    ));
    Panel::new(found_lines(inputs), choices)
}

pub(super) struct ShellStep {
    panel: Panel<ShellAction>,
    inputs: Option<ShellInputs>,
}

impl ShellStep {
    pub(super) fn new() -> Self {
        Self {
            panel: Panel::default(),
            inputs: None,
        }
    }

    fn retitle(&mut self) {
        let name = self.panel.input_value();
        let Some(inputs) = self.inputs.as_mut() else {
            return;
        };
        if name.is_empty() || name == inputs.name {
            return;
        }
        inputs.name = name;
        let cursor = self.panel.cursor;
        self.panel = plan_panel(inputs);
        self.panel.cursor = cursor.min(self.panel.choices.len().saturating_sub(1));
    }

    async fn execute(&mut self, cx: &mut Cx<'_>) {
        let Some(choice) = self.panel.selected() else {
            return;
        };
        let ShellAction::Add(shells) = choice.action.clone() else {
            self.panel.record(vec![crate::finding::Finding::note(
                "No function was added. Start Claude Code with `mike claude`.",
            )]);
            return;
        };
        let name = self.panel.input_value();
        let route = stored_route(cx.env.config_file())
            .ok()
            .flatten()
            .unwrap_or(Route::Sidecar);
        let request = ShellRequest {
            name,
            shells,
            route,
            proxy_addr: crate::doctor::connect_addr(&cx.config.proxy_listen_addr),
        };
        let exe = std::env::current_exe().ok();
        let mut findings = match add_function(&cx.env.locations, &request, exe.as_deref()) {
            Ok(findings) => findings,
            Err(e) => failure(e),
        };
        if crate::finding::all_passed(&findings) {
            findings.push(crate::finding::Finding::note(format!(
                "Open a new terminal, or run `source` on the file above, to use `{}`.",
                request.name
            )));
        }
        self.panel.record(findings);
    }
}

#[async_trait(?Send)]
impl WizardStep for ShellStep {
    fn names(&self) -> &'static [&'static str] {
        &["claude"]
    }

    fn label(&self) -> &'static str {
        "claude code"
    }

    async fn enter(&mut self, cx: &mut Cx<'_>) {
        let locations = &cx.env.locations;
        let installed = Shell::ALL
            .into_iter()
            .filter(|s| s.installed(locations.path_var.as_deref()))
            .map(|s| {
                let rc = s.rc_path(locations, std::env::consts::OS);
                let present = std::fs::read_to_string(&rc)
                    .is_ok_and(|text| text.contains(crate::claude_code::BLOCK_BEGIN));
                (s, rc, present)
            })
            .collect();
        let saved = read_setting(cx.env.config_file(), FUNCTION_KEY)
            .ok()
            .flatten();
        let exe = std::env::current_exe().ok();
        let inputs = ShellInputs {
            installed,
            login: Shell::from_login_shell(std::env::var("SHELL").ok().as_deref()),
            route: stored_route(cx.env.config_file())
                .ok()
                .flatten()
                .unwrap_or(Route::Sidecar),
            name: saved.unwrap_or_else(|| suggest_function_name(locations.path_var.as_deref())),
            proxy_addr: crate::doctor::connect_addr(&cx.config.proxy_listen_addr),
            mike: mike_command(locations, exe.as_deref()),
        };
        self.panel = plan_panel(&inputs);
        self.inputs = Some(inputs);
    }

    async fn key(&mut self, key: KeyEvent, cx: &mut Cx<'_>) -> Outcome {
        let was_editing = self.panel.editing;
        let event = self.panel.handle(key);
        if was_editing && !self.panel.editing {
            self.retitle();
        }
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
        self.panel.draw(frame, area, "claude code · shell function");
    }

    fn hints(&self) -> Hints {
        self.panel.hints()
    }

    fn editing(&self) -> bool {
        self.panel.editing
    }
}
