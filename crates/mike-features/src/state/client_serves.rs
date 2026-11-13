pub fn client_serves(state: &crate::state::AppState, key: &str) -> bool {
    crate::feature_toggles::get_toggle_enabled(&state.feature_toggle_overlay.load(), key)
        .unwrap_or(false)
}

#[cfg(any(test, feature = "test-seams"))]
pub fn serve_every_stage_for_tests(state: &crate::state::AppState) {
    let mut toggles = (**state.feature_toggle_overlay.load()).clone();
    for meta in crate::feature_toggles::TOGGLE_META
        .iter()
        .filter(|m| m.key.starts_with("client_serves_"))
    {
        if let Some(flag) = crate::feature_toggles::client_serves_mut(&mut toggles, meta.key) {
            *flag = true;
        }
    }
    state
        .feature_toggle_overlay
        .store(std::sync::Arc::new(toggles));
}
