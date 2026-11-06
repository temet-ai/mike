use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use uuid::Uuid;

use super::{CallQuery, get_proxy_tools, post_proxy_tool, post_weasel_guard};
use crate::config::{AppState, ProxyConfig};
use crate::feature_toggles::ResolvedToggles;
use crate::proxy_tools::answer::{ProxyToolCall, answer_tool_call};
use crate::proxy_tools::{client, declare, roster};
use crate::weasel_intercept::evidence::weasel_armed;

const PROJECT: &str = "/tmp/session-proxy-tools-test";

fn query() -> Query<CallQuery> {
    Query(CallQuery {
        project: PROJECT.to_owned(),
    })
}

#[tokio::test]
async fn non_uuid_session_is_400_on_every_route() -> anyhow::Result<()> {
    let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    crate::feature_toggles::serve_every_stage_for_tests(&state);
    let bad = || Path("not-a-uuid".to_owned());
    assert_eq!(
        get_proxy_tools(State(state.clone()), bad()).await.err(),
        Some(StatusCode::BAD_REQUEST)
    );
    assert_eq!(
        post_proxy_tool(
            State(state.clone()),
            Path(("not-a-uuid".to_owned(), roster::TOOL_NAME.to_owned())),
            query(),
            None
        )
        .await
        .err(),
        Some(StatusCode::BAD_REQUEST)
    );
    assert_eq!(
        post_weasel_guard(State(state), bad()).await.err(),
        Some(StatusCode::BAD_REQUEST)
    );
    Ok(())
}

#[tokio::test]
async fn listed_tools_equal_the_declared_ones_and_mark_the_session() -> anyhow::Result<()> {
    let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    crate::feature_toggles::serve_every_stage_for_tests(&state);
    let id = Uuid::new_v4().to_string();
    assert!(!client::served_by_client(&id));
    let Ok(Json(answer)) = get_proxy_tools(State(state), Path(id.clone())).await else {
        anyhow::bail!("a UUID session must be answered");
    };
    assert_eq!(answer.tools, declare::declared_tool_values());
    assert!(client::served_by_client(&id));
    assert!(!client::served_by_client(&Uuid::new_v4().to_string()));
    Ok(())
}

#[tokio::test]
async fn a_call_answers_what_the_in_stream_answer_would() -> anyhow::Result<()> {
    let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    crate::feature_toggles::serve_every_stage_for_tests(&state);
    let id = Uuid::new_v4().to_string();
    let Ok(Json(answer)) = post_proxy_tool(
        State(state.clone()),
        Path((id, roster::TOOL_NAME.to_owned())),
        query(),
        None,
    )
    .await
    else {
        anyhow::bail!("a known tool must be answered");
    };
    let call = ProxyToolCall {
        id: "toolu_x".to_owned(),
        name: roster::TOOL_NAME.to_owned(),
        input_json: String::new(),
    };
    assert_eq!(
        answer.content,
        answer_tool_call(&state, &call, PROJECT).await
    );
    Ok(())
}

#[tokio::test]
async fn an_unknown_tool_is_404() -> anyhow::Result<()> {
    let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    crate::feature_toggles::serve_every_stage_for_tests(&state);
    let answer = post_proxy_tool(
        State(state),
        Path((Uuid::new_v4().to_string(), "mike_nope".to_owned())),
        query(),
        None,
    )
    .await;
    assert_eq!(answer.err(), Some(StatusCode::NOT_FOUND));
    Ok(())
}

#[tokio::test]
async fn weasel_is_armed_only_for_a_session_its_client_did_not_guard() -> anyhow::Result<()> {
    let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    crate::feature_toggles::serve_every_stage_for_tests(&state);
    let mut toggles: ResolvedToggles =
        (*state.resolve_session_toggles("weasel-guard-test")).clone();
    toggles.weasel_intercept = true;
    toggles.weasel_intercept_forbidden_words = vec!["pre-existing".to_owned()];
    let guarded = Uuid::new_v4().to_string();
    let other = Uuid::new_v4().to_string();
    assert!(weasel_armed(&toggles, &guarded));
    assert_eq!(
        post_weasel_guard(State(state.clone()), Path(guarded.clone()))
            .await
            .ok(),
        Some(StatusCode::NO_CONTENT)
    );
    assert!(!weasel_armed(&toggles, &guarded));
    assert!(weasel_armed(&toggles, &other));
    Ok(())
}
