use super::*;

fn toggles_of<'a>(presets: &'a [FeaturePreset], id: &str) -> &'a [PresetToggle] {
    presets
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.toggles.as_slice())
        .unwrap_or_default()
}

fn enabled(toggles: &[PresetToggle], key: &str) -> Option<bool> {
    toggles.iter().find(|t| t.key == key).map(|t| t.enabled)
}

#[test]
fn feature_presets_list_the_three_choices_in_the_wizard_order() {
    let ids: Vec<&str> = feature_presets().iter().map(|p| p.id).collect();
    assert_eq!(ids, vec!["watch-only", "recommended", "everything"]);
}

#[test]
fn recommended_turns_on_recall_lessons_and_reminders_over_watch_only() {
    let presets = feature_presets();
    let watch = toggles_of(&presets, "watch-only");
    let recommended = toggles_of(&presets, "recommended");
    for key in [
        "recall_injector",
        "agent_lessons",
        "reminders",
        "code_findings",
    ] {
        assert_eq!(enabled(recommended, key), Some(true), "{key}");
    }
    assert_eq!(enabled(watch, "recall_injector"), Some(false));
}

#[test]
fn everything_turns_on_every_switch_and_watch_only_leaves_session_lessons_off() {
    let presets = feature_presets();
    assert!(toggles_of(&presets, "everything").iter().all(|t| t.enabled));
    let watch = presets.iter().find(|p| p.id == "watch-only");
    assert_eq!(watch.map(|p| p.session_lessons), Some(false));
}

#[tokio::test]
async fn setup_state_route_returns_the_doctor_report_and_the_presets() -> anyhow::Result<()> {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let state = AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
    let res = crate::api::manifest::inspect_router(state)
        .oneshot(
            Request::builder()
                .uri("/api/setup/state")
                .header(axum::http::header::HOST, "127.0.0.1:9989")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await?;
    let body: serde_json::Value = serde_json::from_slice(&bytes)?;
    let checks = body["doctor"]["checks"].as_array().map(Vec::len);
    assert!(checks.is_some_and(|n| n > 0), "{body}");
    assert!(body["doctor"]["version"].is_string(), "{body}");
    assert_eq!(body["feature_presets"].as_array().map(Vec::len), Some(3));
    Ok(())
}
