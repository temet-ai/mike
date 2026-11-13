pub use crate::web_base::client_serves::client_serves;

#[cfg(any(test, feature = "test-seams"))]
pub fn serve_every_stage_for_tests(state: &crate::base_state::BaseState) {
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
