use crossterm::event::{KeyCode, KeyEvent};

use crate::config::ProxyConfig;
use crate::tui::client::ApiClient;
use crate::tui::screens::Nav;

use super::SetupState;
use super::step::Cx;

pub async fn handle_key(
    state: &mut SetupState,
    key: KeyEvent,
    client: &ApiClient,
    config: &mut ProxyConfig,
) -> Nav {
    if !state.steps[state.current].editing() {
        match key.code {
            KeyCode::Char('q') => return Nav::Quit,
            KeyCode::Esc => return Nav::SetupDone,
            _ => {}
        }
    }
    let outcome = {
        let mut cx = Cx {
            env: &mut state.env,
            client,
            config,
        };
        state.steps[state.current].key(key, &mut cx).await
    };
    state.apply(outcome, client, config).await
}
