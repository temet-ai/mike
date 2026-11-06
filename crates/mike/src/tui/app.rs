use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::widgets::ListState;

use crate::config::ProxyConfig;
use crate::stack;

use super::client::ApiClient;
use super::input::is_shortcuts_toggle;
use super::screens::{
    Nav, Screen, config, home, request_detail, requests, sessions, setup, start_stop,
};
use super::widgets;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingExec {
    None,
    Claude,
}

pub struct App {
    config: ProxyConfig,
    client: ApiClient,
    screen: Screen,
    nav: ListState,
    nav_focus: bool,
    running: bool,
    show_shortcuts: bool,
    should_quit: bool,
    pending_exec: PendingExec,
}

impl App {
    pub fn new(config: ProxyConfig) -> Self {
        if super::needs_setup(&config) || super::setup_unfinished(&config) {
            return Self::with_setup(config, setup::SetupState::new());
        }
        Self::with_screen(config, Screen::Home(home::HomeState::new()), 0, true)
    }

    pub fn with_setup(config: ProxyConfig, mut state: setup::SetupState) -> Self {
        state.select_config(&config);
        Self::with_screen(config, Screen::Setup(Box::new(state)), 2, false)
    }

    fn with_screen(config: ProxyConfig, screen: Screen, nav_index: usize, nav_focus: bool) -> Self {
        let client = ApiClient::new(&config);
        let mut nav = ListState::default();
        nav.select(Some(nav_index));
        Self {
            config,
            client,
            screen,
            nav,
            nav_focus,
            running: false,
            show_shortcuts: false,
            should_quit: false,
            pending_exec: PendingExec::None,
        }
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn pending_exec(&self) -> PendingExec {
        self.pending_exec
    }

    pub async fn on_enter_screen(&mut self) {
        self.running = stack::probe_selected(&self.config).await;
        match &mut self.screen {
            Screen::Home(s) => s.refresh(&self.client, &self.config).await,
            Screen::StartStop(s) => s.refresh(&self.client, &self.config).await,
            Screen::Config(s) => s.refresh(&self.client).await,
            Screen::Setup(s) => s.on_enter_step(&self.client, &mut self.config).await,
            Screen::Sessions(s) => s.refresh(&self.client).await,
            Screen::Requests(s) => s.refresh(&self.client).await,
            Screen::RequestDetail(s) => s.refresh(&self.client).await,
        }
    }

    pub async fn on_tick(&mut self) {
        self.running = stack::probe_selected(&self.config).await;
        match &mut self.screen {
            Screen::Home(s) => s.refresh(&self.client, &self.config).await,
            Screen::StartStop(s) => s.refresh(&self.client, &self.config).await,
            Screen::Setup(s) => s.on_tick(&self.client, &mut self.config).await,
            Screen::Config(_) => {}
            Screen::Sessions(_) | Screen::Requests(_) | Screen::RequestDetail(_) => {}
        }
    }

    pub fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(3)])
            .split(area);
        widgets::render_top_bar(frame, chunks[0], self.running);

        let show_nav = !matches!(
            self.screen,
            Screen::StartStop(_) | Screen::Requests(_) | Screen::RequestDetail(_)
        );
        let content_area = if show_nav {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(widgets::NAV_WIDTH), Constraint::Min(0)])
                .split(chunks[1]);
            let rail_rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(widgets::NAV_HEIGHT), Constraint::Min(0)])
                .split(cols[0]);
            widgets::render_nav_rail(frame, rail_rows[0], &mut self.nav, self.nav_focus);
            cols[1]
        } else {
            chunks[1]
        };
        match &mut self.screen {
            Screen::Home(s) => home::draw(frame, content_area, s),
            Screen::Setup(s) => setup::draw(frame, content_area, s),
            Screen::StartStop(s) => start_stop::draw(frame, content_area, s),
            Screen::Config(s) => config::draw(frame, content_area, s),
            Screen::Sessions(s) => sessions::draw(frame, content_area, s),
            Screen::Requests(s) => requests::draw(frame, content_area, s),
            Screen::RequestDetail(s) => request_detail::draw(frame, content_area, s),
        }
        if self.show_shortcuts {
            widgets::render_shortcuts_modal(frame, area, &self.screen.shortcuts());
        }
    }

    pub async fn handle_key(&mut self, key: KeyEvent) {
        let text_input = !self.nav_focus
            && matches!(&self.screen, Screen::Setup(state) if state.accepts_text_input());
        if is_shortcuts_toggle(&key, text_input && !self.show_shortcuts) {
            self.show_shortcuts = !self.show_shortcuts;
            return;
        }
        if self.show_shortcuts {
            if key.code == KeyCode::Esc {
                self.show_shortcuts = false;
            }
            return;
        }

        let show_nav = !matches!(self.screen, Screen::StartStop(_));

        if show_nav && self.nav_focus {
            self.handle_nav_focused_key(key).await;
            return;
        }

        if show_nav && key.code == KeyCode::BackTab {
            self.nav_focus = true;
            return;
        }

        let nav = self.dispatch_screen_key(key).await;
        self.apply_nav(nav).await;
    }

    async fn handle_nav_focused_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Tab | KeyCode::Enter => self.nav_focus = false,
            KeyCode::Char('j') | KeyCode::Down => {
                move_nav(&mut self.nav, 1);
                self.sync_screen_to_nav().await;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                move_nav(&mut self.nav, -1);
                self.sync_screen_to_nav().await;
            }
            KeyCode::Char('g') => {
                self.nav.select(Some(0));
                self.sync_screen_to_nav().await;
            }
            KeyCode::Char('G') => {
                self.nav.select(Some(widgets::NAV_ITEMS.len() - 1));
                self.sync_screen_to_nav().await;
            }
            _ => {}
        }
    }

    async fn dispatch_screen_key(&mut self, key: KeyEvent) -> Nav {
        match &mut self.screen {
            Screen::Home(s) => home::handle_key(s, key, &self.client, &self.config).await,
            Screen::Setup(s) => setup::handle_key(s, key, &self.client, &mut self.config).await,
            Screen::StartStop(s) => {
                start_stop::handle_key(s, key, &self.client, &self.config).await
            }
            Screen::Config(s) => config::handle_key(s, key, &self.client).await,
            Screen::Sessions(s) => sessions::handle_key(s, key, &self.client).await,
            Screen::Requests(s) => requests::handle_key(s, key, &self.client).await,
            Screen::RequestDetail(s) => request_detail::handle_key(s, key, &self.client).await,
        }
    }

    async fn sync_screen_to_nav(&mut self) {
        match (self.nav.selected().unwrap_or(0), &self.screen) {
            (0, Screen::Home(_))
            | (1, Screen::Config(_))
            | (2, Screen::Setup(_))
            | (3, Screen::Sessions(_)) => return,
            (0, _) => self.screen = Screen::Home(home::HomeState::new()),
            (1, _) => self.screen = Screen::Config(config::ConfigState::new()),
            (2, _) => {
                self.screen = Screen::Setup(Box::new(setup::SetupState::for_config(&self.config)))
            }
            (3, _) => self.screen = Screen::Sessions(sessions::SessionsState::new()),
            _ => return,
        }
        self.on_enter_screen().await;
    }

    async fn apply_nav(&mut self, nav: Nav) {
        match nav {
            Nav::None => {}
            Nav::Quit => self.should_quit = true,
            Nav::Home => {
                self.screen = Screen::Home(home::HomeState::new());
                self.nav.select(Some(0));
                self.nav_focus = true;
                self.on_enter_screen().await;
            }
            Nav::Setup => {
                self.screen = Screen::Setup(Box::new(setup::SetupState::for_config(&self.config)));
                self.nav.select(Some(2));
                self.nav_focus = true;
                self.on_enter_screen().await;
            }
            Nav::StartStop => {
                self.screen = Screen::StartStop(start_stop::StartStopState::new());
                self.on_enter_screen().await;
            }
            Nav::SetupDone => {
                self.screen = Screen::Home(home::HomeState::new());
                self.nav.select(Some(0));
                self.nav_focus = true;
                self.on_enter_screen().await;
            }
            Nav::LaunchClaude => {
                self.pending_exec = PendingExec::Claude;
                self.should_quit = true;
            }
            Nav::Sessions => {
                self.screen = Screen::Sessions(sessions::SessionsState::new());
                self.nav.select(Some(3));
                self.nav_focus = true;
                self.on_enter_screen().await;
            }
            Nav::OpenSessionRequests(session_id) => {
                self.screen = Screen::Requests(requests::RequestsState::new(session_id));
                self.on_enter_screen().await;
            }
            Nav::OpenRequestDetail {
                session_id,
                request_id,
            } => {
                self.screen = Screen::RequestDetail(Box::new(
                    request_detail::RequestDetailState::new(session_id, request_id),
                ));
                self.on_enter_screen().await;
            }
        }
    }
}

fn move_nav(list_state: &mut ListState, delta: i32) {
    let len = widgets::NAV_ITEMS.len() as i32;
    let current = list_state.selected().unwrap_or(0) as i32;
    let next = (current + delta).rem_euclid(len) as usize;
    list_state.select(Some(next));
}

#[cfg(test)]
mod tests;
