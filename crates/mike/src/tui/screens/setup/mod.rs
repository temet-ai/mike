mod facts;
#[cfg(test)]
mod harness;
mod keys;
mod panel;
mod persist;
mod render;
mod report;
mod step;
mod steps;
#[cfg(test)]
mod tests;

pub use keys::handle_key;
pub use render::{draw, shortcuts};

use crate::claude_code::{SETUP_KEY, read_setting, write_setting};
use crate::config::ProxyConfig;
use crate::tui::client::ApiClient;
use crate::tui::screens::Nav;
use step::{Cx, Env, Outcome, WizardStep};

pub const UNFINISHED: &str = "unfinished";
pub const DONE: &str = "done";

pub struct SetupState {
    steps: Vec<Box<dyn WizardStep>>,
    current: usize,
    env: Env,
    started: bool,
}

pub fn marked_unfinished(value: Option<&str>) -> bool {
    value.is_some_and(|v| v.trim().eq_ignore_ascii_case(UNFINISHED))
}

impl SetupState {
    pub(crate) fn for_config(config: &ProxyConfig) -> Self {
        let mut state = Self::new();
        state.select_config(config);
        state
    }

    pub(crate) fn select_config(&mut self, config: &ProxyConfig) {
        self.env.locations.config_file = config.config_path.clone();
    }

    pub(crate) fn accepts_text_input(&self) -> bool {
        self.steps[self.current].editing()
    }

    pub fn new() -> Self {
        Self::with_parts(Env::from_process(), steps::all(), 0)
    }

    fn with_parts(env: Env, steps: Vec<Box<dyn WizardStep>>, current: usize) -> Self {
        Self {
            steps,
            current,
            env,
            started: false,
        }
    }

    pub fn at_step(name: &str) -> Result<Self, String> {
        let steps = steps::all();
        let found = steps.iter().position(|step| {
            step.names()
                .contains(&name.trim().to_ascii_lowercase().as_str())
        });
        match found {
            Some(index) => Ok(Self::with_parts(Env::from_process(), steps, index)),
            None => Err(format!(
                "`{name}` is not a setup step. The steps are {}",
                Self::step_names().join(", ")
            )),
        }
    }

    pub fn step_names() -> Vec<&'static str> {
        steps::all()
            .iter()
            .flat_map(|step| step.names().iter().copied())
            .collect()
    }

    pub async fn on_enter_step(&mut self, client: &ApiClient, config: &mut ProxyConfig) {
        if !self.started {
            self.started = true;
            self.mark_started();
        }
        self.enter_current(client, config).await;
    }

    pub async fn on_tick(&mut self, client: &ApiClient, config: &mut ProxyConfig) {
        let mut cx = Cx {
            env: &mut self.env,
            client,
            config,
        };
        self.steps[self.current].tick(&mut cx).await;
    }

    async fn enter_current(&mut self, client: &ApiClient, config: &mut ProxyConfig) {
        let mut cx = Cx {
            env: &mut self.env,
            client,
            config,
        };
        self.steps[self.current].enter(&mut cx).await;
    }

    fn remember(&self, value: &str) {
        if let Err(e) = write_setting(self.env.config_file(), SETUP_KEY, value) {
            tracing::warn!(error = %e, value, "setup: could not record the setup state");
        }
    }

    fn mark_started(&self) {
        let known = read_setting(self.env.config_file(), SETUP_KEY)
            .ok()
            .flatten();
        if known.is_none() {
            self.remember(UNFINISHED);
        }
    }

    async fn advance(&mut self, client: &ApiClient, config: &mut ProxyConfig) -> Nav {
        if self.current + 1 >= self.steps.len() {
            return Nav::SetupDone;
        }
        self.current += 1;
        if self.current + 1 == self.steps.len() {
            self.remember(DONE);
        }
        self.enter_current(client, config).await;
        Nav::None
    }

    async fn apply(
        &mut self,
        outcome: Outcome,
        client: &ApiClient,
        config: &mut ProxyConfig,
    ) -> Nav {
        match outcome {
            Outcome::Stay => Nav::None,
            Outcome::Next => self.advance(client, config).await,
            Outcome::Back => {
                if self.current > 0 {
                    self.current -= 1;
                    self.enter_current(client, config).await;
                }
                Nav::None
            }
            Outcome::FinishLater => Nav::SetupDone,
            Outcome::Leave(nav) => nav,
        }
    }
}

impl Default for SetupState {
    fn default() -> Self {
        Self::new()
    }
}
