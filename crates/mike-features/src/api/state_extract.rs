use axum::extract::FromRef;

use crate::base_state::BaseState;
use crate::launch::state::LaunchState;
use crate::state::AppState;

impl FromRef<AppState> for BaseState {
    fn from_ref(state: &AppState) -> BaseState {
        state.base.clone()
    }
}

impl FromRef<AppState> for LaunchState {
    fn from_ref(state: &AppState) -> LaunchState {
        state.launch_state()
    }
}
