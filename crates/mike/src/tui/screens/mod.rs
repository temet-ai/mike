pub mod config;
pub mod home;
pub mod request_detail;
pub mod requests;
pub mod sessions;
pub mod setup;
pub mod start_stop;

use crate::tui::widgets::ShortcutsSection;

pub enum Screen {
    Setup(Box<setup::SetupState>),
    Home(home::HomeState),
    StartStop(start_stop::StartStopState),
    Config(config::ConfigState),
    Sessions(sessions::SessionsState),
    Requests(requests::RequestsState),
    RequestDetail(Box<request_detail::RequestDetailState>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Nav {
    None,
    Quit,
    Home,
    Setup,
    StartStop,
    SetupDone,
    LaunchClaude,
    Sessions,
    OpenSessionRequests(String),
    OpenRequestDetail { session_id: String, request_id: u64 },
}

impl Screen {
    pub fn shortcuts(&self) -> Vec<ShortcutsSection> {
        match self {
            Screen::Home(_) => home::shortcuts(),
            Screen::Setup(s) => setup::shortcuts(s),
            Screen::StartStop(_) => start_stop::shortcuts(),
            Screen::Config(_) => config::shortcuts(),
            Screen::Sessions(_) => sessions::shortcuts(),
            Screen::Requests(_) => requests::shortcuts(),
            Screen::RequestDetail(_) => request_detail::shortcuts(),
        }
    }
}

pub(super) fn request_status_glyph(status: &str) -> (char, ratatui::style::Color, &'static str) {
    use crate::tui::theme::{self, glyph};
    let lower = status.to_lowercase();
    if lower.starts_with("complete") {
        (glyph::FILLED_CIRCLE, theme::ACID, "complete")
    } else if lower.starts_with("streaming") {
        (glyph::CURRENT_STEP, theme::ICE, "streaming")
    } else if lower.starts_with("pending") {
        (glyph::OPEN_CIRCLE, theme::FG_3, "pending")
    } else {
        (glyph::OPEN_CIRCLE, theme::RED, "error")
    }
}
