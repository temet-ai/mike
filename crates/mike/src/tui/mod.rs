mod app;
pub(crate) mod client;
mod input;
mod screens;
#[cfg(test)]
pub(crate) mod test_support;
mod theme;
mod widgets;

use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::config::{CliOverrides, ProxyConfig};

type Term = Terminal<CrosstermBackend<io::Stdout>>;

const TICK_RATE: Duration = Duration::from_millis(250);

pub async fn run(overrides: CliOverrides) -> anyhow::Result<()> {
    let config = crate::config::load(overrides.clone())?;
    drive(app::App::new(config), overrides).await
}

pub async fn run_setup(overrides: CliOverrides, step: Option<String>) -> anyhow::Result<()> {
    use std::io::IsTerminal;
    let state = match step {
        Some(name) => screens::setup::SetupState::at_step(&name).map_err(anyhow::Error::msg)?,
        None => screens::setup::SetupState::new(),
    };
    if !io::stdout().is_terminal() {
        anyhow::bail!(
            "`mike setup` needs a terminal. Run it in one, or use `mike doctor` to see what is missing"
        );
    }
    let config = crate::config::load(overrides.clone())?;
    drive(app::App::with_setup(config, state), overrides).await
}

async fn drive(mut app: app::App, overrides: CliOverrides) -> anyhow::Result<()> {
    install_panic_hook();
    let mut terminal = enter_terminal()?;
    let result = run_event_loop(&mut terminal, &mut app).await;
    leave_terminal(&mut terminal)?;
    result?;

    match app.pending_exec() {
        app::PendingExec::None => Ok(()),
        app::PendingExec::Claude => crate::claude::run(Vec::new(), overrides).await,
    }
}

pub(crate) fn needs_setup(config: &ProxyConfig) -> bool {
    config.persist_enabled && config.database_url.is_none()
}

pub(crate) fn setup_unfinished(config: &ProxyConfig) -> bool {
    let value =
        crate::claude_code::read_setting(&config.config_path, crate::claude_code::SETUP_KEY)
            .ok()
            .flatten();
    screens::setup::marked_unfinished(value.as_deref())
}

pub(crate) use client::tcp_reachable;

pub(crate) fn on_path(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(bin).is_file()))
        .unwrap_or(false)
}

fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original(info);
    }));
}

fn enter_terminal() -> anyhow::Result<Term> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

fn leave_terminal(terminal: &mut Term) -> anyhow::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

async fn run_event_loop(terminal: &mut Term, app: &mut app::App) -> anyhow::Result<()> {
    app.on_enter_screen().await;
    loop {
        terminal.draw(|frame| app.draw(frame))?;
        if event::poll(TICK_RATE)? {
            if let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                app.handle_key(key).await;
            }
        } else {
            app.on_tick().await;
        }
        if app.should_quit() {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_setup_when_persistence_on_and_no_database_url() {
        let config = ProxyConfig::for_tests_in_memory();
        let mut config = config;
        config.persist_enabled = true;
        config.database_url = None;
        assert!(needs_setup(&config));
    }

    #[test]
    fn setup_not_needed_once_a_database_url_is_configured() {
        let mut config = ProxyConfig::for_tests_in_memory();
        config.persist_enabled = true;
        config.database_url = Some("postgres://127.0.0.1:5432/mike".to_string());
        assert!(!needs_setup(&config));
    }

    #[test]
    fn an_unfinished_setup_reopens_even_with_a_database_configured() {
        assert!(screens::setup::marked_unfinished(Some("unfinished")));
        assert!(!screens::setup::marked_unfinished(Some("done")));
        assert!(!screens::setup::marked_unfinished(None));
    }

    #[test]
    fn setup_not_needed_once_trial_mode_is_recorded() {
        let mut config = ProxyConfig::for_tests_in_memory();
        config.persist_enabled = false;
        config.database_url = None;
        assert!(!needs_setup(&config));
    }
}
