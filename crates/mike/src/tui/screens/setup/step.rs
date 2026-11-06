use std::path::Path;

use async_trait::async_trait;
use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;

use super::facts::Facts;
use crate::claude_code::Locations;
use crate::config::ProxyConfig;
use crate::tui::client::ApiClient;
use crate::tui::screens::Nav;

pub(super) struct Env {
    pub(super) locations: Locations,
    pub(super) facts: Facts,
    pub(super) looked: bool,
}

impl Env {
    pub(super) fn from_process() -> Self {
        let locations = Locations::from_environment().unwrap_or_else(|_| {
            let mut fallback = Locations::for_home(Path::new(""), std::env::var_os("PATH"));
            fallback.config_file = crate::user_config::resolved_config_file().unwrap_or_default();
            fallback
        });
        Self::with_locations(locations)
    }

    pub(super) fn with_locations(locations: Locations) -> Self {
        Self {
            locations,
            facts: Facts::default(),
            looked: true,
        }
    }

    pub(super) fn config_file(&self) -> &Path {
        &self.locations.config_file
    }
}

pub(super) struct Cx<'a> {
    pub(super) env: &'a mut Env,
    pub(super) client: &'a ApiClient,
    pub(super) config: &'a mut ProxyConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Outcome {
    Stay,
    Next,
    Back,
    FinishLater,
    Leave(Nav),
}

pub(super) type Hints = Vec<(&'static str, &'static str)>;

#[async_trait(?Send)]
pub(super) trait WizardStep {
    fn names(&self) -> &'static [&'static str];

    fn label(&self) -> &'static str;

    async fn enter(&mut self, cx: &mut Cx<'_>);

    async fn key(&mut self, key: KeyEvent, cx: &mut Cx<'_>) -> Outcome;

    fn draw(&self, frame: &mut Frame, area: Rect);

    fn hints(&self) -> Hints;

    fn editing(&self) -> bool {
        false
    }

    fn banner(&self) -> Option<String> {
        None
    }

    fn progress(&self) -> Option<(usize, usize)> {
        None
    }

    async fn tick(&mut self, _cx: &mut Cx<'_>) {}
}
