use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use bytes::Bytes;
use uuid::Uuid;

use super::{ProjectQuery, PromptShaping, get_prompt_shaping};
use crate::compose_prompt::stage::{TurnIdentity, run};
use crate::config::ProxyConfig;
use crate::extract::request::TurnOrigin;
use crate::filter::stage::FilterStage;
use crate::filter::{FilterAction, FilterConfig, FilterKind, FilterRule, RuleSource};
use crate::lazy_sections::stage::LazySectionsStage;
use crate::lesson_worker::LessonSnapshot;
use crate::pipeline::{RequestStage, StageBody, StageContext};
use crate::state::AppState;
use crate::store::models::RequestId;
use crate::wire::manifest::ManifestBuilder;

const BODY: &[u8] =
    br#"{"system":[{"type":"text","text":"keep BANNED-PHRASE here"}],"messages":[]}"#;
const PROJECT: &str = "/work/project-a";

fn rule(id: i64, kind: FilterKind, value: &str, project: Option<&str>) -> FilterRule {
    FilterRule {
        id,
        uuid: Uuid::new_v4(),
        kind,
        value: value.to_owned(),
        enabled: true,
        source: RuleSource::Ui,
        updated_at: chrono::Utc::now(),
        sort_order: 0,
        action: FilterAction::Drop,
        text: None,
        trigger_token: None,
        inject_text: None,
        project_path: project.map(str::to_owned),
        style_override: false,
        title: None,
    }
}

async fn state_with(rules: Vec<FilterRule>) -> anyhow::Result<AppState> {
    let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    crate::state::serve_every_stage_for_tests(&state);
    state
        .filter_config
        .store(std::sync::Arc::new(FilterConfig { rules }));
    Ok(state)
}

async fn call(state: &AppState, id: &str, project: &str) -> Result<PromptShaping, StatusCode> {
    let query = Query(ProjectQuery {
        project: project.to_owned(),
    });
    get_prompt_shaping(State(state.clone()), Path(id.to_owned()), query)
        .await
        .map(|Json(shaping)| shaping)
}

async fn reason_of_filter(state: &AppState, session: &str) -> anyhow::Result<(String, bool)> {
    let toggles = state.resolve_session_toggles(session);
    let cfg = state.filter_config.load_full();
    let lessons = LessonSnapshot::default();
    let origin = TurnOrigin::PrimaryFirst;
    let ctx = StageContext {
        state,
        session_toggles: &toggles,
        filter_cfg: &cfg,
        lesson_snapshot: &lessons,
        session_id: session,
        project_path: PROJECT,
        turn_origin: &origin,
        request_id: RequestId::next(),
        is_tool_loop: false,
        subagent_persona: None,
        has_agent_id: false,
    };
    anyhow::ensure!(FilterStage.enabled(&ctx), "filter stage must be enabled");
    let mut body = StageBody::new(Bytes::from_static(BODY), "orig".to_owned());
    FilterStage.apply(&ctx, &mut body).await;
    let rewrote = body.body().as_ref() != BODY;
    let manifest = body.into_parts().manifest.finish("orig".to_owned());
    let entry = manifest
        .stages
        .first()
        .ok_or_else(|| anyhow::anyhow!("no manifest entry"))?;
    Ok((entry.reason_code.clone(), rewrote))
}

async fn reason_of_lazy(state: &AppState, session: &str) -> anyhow::Result<String> {
    let toggles = state.resolve_session_toggles(session);
    let cfg = state.filter_config.load_full();
    let lessons = LessonSnapshot::default();
    let origin = TurnOrigin::Primary;
    let ctx = StageContext {
        state,
        session_toggles: &toggles,
        filter_cfg: &cfg,
        lesson_snapshot: &lessons,
        session_id: session,
        project_path: PROJECT,
        turn_origin: &origin,
        request_id: RequestId::next(),
        is_tool_loop: false,
        subagent_persona: None,
        has_agent_id: false,
    };
    let mut body = StageBody::new(Bytes::from_static(BODY), "orig".to_owned());
    LazySectionsStage.apply(&ctx, &mut body).await;
    let manifest = body.into_parts().manifest.finish("orig".to_owned());
    let entry = manifest
        .stages
        .first()
        .ok_or_else(|| anyhow::anyhow!("no manifest entry"))?;
    Ok(entry.reason_code.clone())
}

async fn reason_of_compose(state: &AppState, _session: &str) -> anyhow::Result<String> {
    let mut manifest = ManifestBuilder::new("orig".to_owned());
    let origin = TurnOrigin::PrimaryFirst;
    let body = Bytes::from_static(BODY);
    run(
        state,
        body.clone(),
        &body,
        TurnIdentity {
            turn_origin: &origin,
            class_key: Some(crate::compose_prompt::PromptClassKey::Main),
            is_count_tokens_probe: false,
            mutation_exempt: false,
        },
        RequestId::next(),
        &mut manifest,
    )
    .await;
    let finished = manifest.finish("orig".to_owned());
    let entry = finished
        .stages
        .first()
        .ok_or_else(|| anyhow::anyhow!("no manifest entry"))?;
    Ok(entry.reason_code.clone())
}

#[tokio::test]
async fn prompt_shaping_non_uuid_session_is_400() -> anyhow::Result<()> {
    let state = state_with(Vec::new()).await?;
    let answer = call(&state, "not-a-uuid", PROJECT).await;
    assert_eq!(answer.err(), Some(StatusCode::BAD_REQUEST));
    Ok(())
}

#[tokio::test]
async fn prompt_shaping_answers_the_project_rules_and_omits_another_projects() -> anyhow::Result<()>
{
    let mut lazy = rule(3, FilterKind::LazySection, "# Chrome", None);
    lazy.action = FilterAction::Replace;
    lazy.text = Some("stub".to_owned());
    lazy.trigger_token = Some("!!T!!".to_owned());
    let state = state_with(vec![
        rule(1, FilterKind::TextSubstring, "mine", Some(PROJECT)),
        rule(2, FilterKind::XmlTag, "other", Some("/work/project-b")),
        lazy,
        rule(4, FilterKind::XmlTag, "global", None),
    ])
    .await?;
    let id = Uuid::new_v4().to_string();
    let shaping = call(&state, &id, PROJECT)
        .await
        .map_err(|s| anyhow::anyhow!("status {s}"))?;
    let json = serde_json::to_value(&shaping)?;
    let rules = json["filter_rules"].as_array().cloned().unwrap_or_default();
    let ids: Vec<i64> = rules.iter().filter_map(|r| r["id"].as_i64()).collect();
    assert_eq!(ids, vec![1, 4], "got {json}");
    assert_eq!(rules[0]["match"], "substring_ci");
    assert_eq!(rules[1]["match"], "xml_block");
    assert_eq!(rules[0]["scope"], "system_and_messages");
    let lazies = json["lazy_sections"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(lazies.len(), 1);
    assert_eq!(lazies[0]["heading"], "# Chrome");
    assert_eq!(lazies[0]["stub"], "stub");
    assert_eq!(lazies[0]["trigger_token"], "!!T!!");
    assert!(json["compositions"].is_array());
    Ok(())
}

#[tokio::test]
async fn a_session_that_read_its_shaping_keeps_every_rule_mike_applies() -> anyhow::Result<()> {
    let mut reminder = rule(2, FilterKind::InjectReminder, "", None);
    reminder.action = FilterAction::Replace;
    reminder.text = Some("ADD-ME".to_owned());
    reminder.trigger_token = Some("!!T!!".to_owned());
    let state = state_with(vec![
        rule(1, FilterKind::TextSubstring, "BANNED-PHRASE", None),
        reminder,
    ])
    .await?;
    let served = Uuid::new_v4().to_string();
    let other = Uuid::new_v4().to_string();

    assert_eq!(
        reason_of_lazy(&state, &other).await?,
        "lazy_sections_no_match"
    );

    call(&state, &served, PROJECT)
        .await
        .map_err(|s| anyhow::anyhow!("status {s}"))?;

    let (reason, rewrote) = reason_of_filter(&state, &served).await?;
    assert_eq!(reason, "filter_rule_matched");
    assert!(
        rewrote,
        "tether applies no text_substring rule, so mike keeps it"
    );
    assert_ne!(
        reason_of_compose(&state, &served).await?,
        "composer_served_by_client"
    );
    assert_eq!(
        reason_of_lazy(&state, &served).await?,
        "lazy_sections_no_match",
        "a reminder fires on an assistant message mid-turn, which no hook sees, so mike keeps it"
    );
    assert_eq!(
        reason_of_lazy(&state, &other).await?,
        "lazy_sections_no_match"
    );
    Ok(())
}
