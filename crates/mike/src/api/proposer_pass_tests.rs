use axum::extract::State;
use axum::http::StatusCode;

use super::{post_proposer_pass, ran_body};
use crate::config::ProxyConfig;
use crate::lesson_proposer::full_pass::FullPass;
use crate::state::AppState;

#[test]
fn ran_body_has_the_outcome_and_each_part_of_the_pass() {
    let body = ran_body(&FullPass::default());
    assert_eq!(body["outcome"], "ran");
    for part in ["proposer", "topics", "skill_author", "skill_eval"] {
        assert!(body[part].is_object(), "{part} is missing");
    }
}

#[tokio::test]
async fn post_proposer_pass_without_persistence_is_unavailable() -> anyhow::Result<()> {
    let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    let response = post_proposer_pass(State(state)).await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    Ok(())
}
