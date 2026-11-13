use super::*;

#[test]
fn stamps_capture_identity_true_only_for_real_turns() {
    assert!(TurnOrigin::Primary.stamps_capture_identity());
    assert!(TurnOrigin::PrimaryFirst.stamps_capture_identity());
    assert!(TurnOrigin::Subagent.stamps_capture_identity());
    assert!(!TurnOrigin::TitleSidecar.stamps_capture_identity());
    assert!(!TurnOrigin::AdvisorSidecar.stamps_capture_identity());
    assert!(!TurnOrigin::QuotaProbe.stamps_capture_identity());
}

#[test]
fn only_calls_that_resend_the_session_tool_list_say_so() {
    for origin in [
        TurnOrigin::Primary,
        TurnOrigin::PrimaryFirst,
        TurnOrigin::Subagent,
        TurnOrigin::WorkflowSubagent,
        TurnOrigin::CountTokensSidecar,
        TurnOrigin::NextInputSuggestionSidecar,
    ] {
        assert!(origin.resends_the_session_tool_list(), "{origin}");
    }
    for origin in [
        TurnOrigin::TitleSidecar,
        TurnOrigin::AdvisorSidecar,
        TurnOrigin::QuotaProbe,
        TurnOrigin::MonitorSidecar,
        TurnOrigin::WebFetchSidecar,
    ] {
        assert!(!origin.resends_the_session_tool_list(), "{origin}");
    }
}

fn all_turn_origins() -> Vec<TurnOrigin> {
    let all = vec![
        TurnOrigin::Primary,
        TurnOrigin::PrimaryFirst,
        TurnOrigin::TitleSidecar,
        TurnOrigin::Subagent,
        TurnOrigin::AdvisorSidecar,
        TurnOrigin::QuotaProbe,
        TurnOrigin::MonitorSidecar,
        TurnOrigin::CountTokensSidecar,
        TurnOrigin::WebFetchSidecar,
        TurnOrigin::WorkflowSubagent,
        TurnOrigin::NextInputSuggestionSidecar,
    ];
    for v in &all {
        match v {
            TurnOrigin::Primary
            | TurnOrigin::PrimaryFirst
            | TurnOrigin::TitleSidecar
            | TurnOrigin::Subagent
            | TurnOrigin::AdvisorSidecar
            | TurnOrigin::QuotaProbe
            | TurnOrigin::MonitorSidecar
            | TurnOrigin::CountTokensSidecar
            | TurnOrigin::WebFetchSidecar
            | TurnOrigin::WorkflowSubagent
            | TurnOrigin::NextInputSuggestionSidecar => {}
        }
    }
    all
}

#[test]
fn every_turn_origin_variant_is_seeded_in_migrations() -> anyhow::Result<()> {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/store/persistence/migrations");
    let mut sql = String::new();
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "sql") {
            sql.push_str(&std::fs::read_to_string(&path)?);
        }
    }
    assert!(!sql.is_empty(), "no migration SQL found in {dir:?}");

    for origin in all_turn_origins() {
        let needle = format!("'{origin}'");
        assert!(
            sql.contains(&needle),
            "TurnOrigin::{origin:?} ({origin}) has no seed row in any migration — \
             the snapshot INSERT's lookup join will silently drop every request \
             stamped with it. Add an INSERT INTO requests.turn_origins migration."
        );
    }
    Ok(())
}

#[test]
fn noise_origins_and_reliable_sidecar_list_agree() {
    use crate::store::persistence::messages::RELIABLE_SIDECAR_ORIGINS;

    for origin in all_turn_origins() {
        let code = origin.to_string();
        let is_noise = !origin.stamps_capture_identity();
        let in_list = RELIABLE_SIDECAR_ORIGINS.contains(&code.as_str());
        assert_eq!(
            is_noise, in_list,
            "TurnOrigin::{origin:?} ({code}): stamps_capture_identity says \
             noise={is_noise} but RELIABLE_SIDECAR_ORIGINS membership is {in_list} — \
             update one side so the two stay in lockstep."
        );
    }
}
