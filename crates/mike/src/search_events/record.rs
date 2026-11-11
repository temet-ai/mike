use serde_json::{Value, json};
use uuid::Uuid;

use super::shown;
use crate::api::handlers::SimilarTurnDto;
use crate::api::rerank_search::RerankReport;
use crate::config::AppState;
use crate::store::persistence::project_knowledge::KnowledgeQuery;
use crate::store::persistence::search_events::NewSearchEvent;

const KNOWLEDGE_TOOL: &str = "search_project_knowledge";
const TURNS_TOOL: &str = "recall_similar_turns";

pub(super) struct Draft {
    pub(super) tool: &'static str,
    pub(super) query: String,
    pub(super) kind_filter: Option<String>,
    pub(super) returned: Value,
    pub(super) rerank_state: &'static str,
}

pub(crate) struct SearchLog(Draft);

impl SearchLog {
    pub(crate) fn knowledge(input: &KnowledgeQuery, answer: &Value) -> Self {
        Self(knowledge_draft(input, answer))
    }

    pub(crate) fn delivered_as(mut self, delivered: &Value) -> Self {
        shown::set_from_items(&mut self.0.returned, &delivered["items"]);
        self
    }

    pub(crate) fn record(self, state: &AppState, session: Option<&str>) {
        spawn_record(state, session, self.0);
    }
}

pub(crate) fn record_turn_recall(
    state: &AppState,
    session: Option<&str>,
    query: &str,
    turns: &[SimilarTurnDto],
    report: &RerankReport,
) {
    spawn_record(state, session, turns_draft(query, turns, report));
}

pub(super) fn knowledge_draft(input: &KnowledgeQuery, answer: &Value) -> Draft {
    let items = answer["items"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let mut returned = Value::Array(items.iter().map(knowledge_entry).collect());
    shown::set_from_items(&mut returned, &answer["items"]);
    Draft {
        tool: KNOWLEDGE_TOOL,
        query: input.query.clone(),
        kind_filter: input.kind.clone(),
        returned,
        rerank_state: state_label(answer["rerank"]["state"].as_str()),
    }
}

fn knowledge_entry(item: &Value) -> Value {
    scored_entry(
        item["reference"].clone(),
        item["rank"].clone(),
        item.get("rerank_score"),
    )
}

pub(super) fn turns_draft(query: &str, turns: &[SimilarTurnDto], report: &RerankReport) -> Draft {
    let entries = turns
        .iter()
        .map(|turn| {
            let rerank_score = turn.rerank_score.map(Value::from);
            scored_entry(
                json!(format!("turn:{}", turn.turn.turn_id)),
                json!(turn.rank),
                rerank_score.as_ref(),
            )
        })
        .collect();
    let mut returned = Value::Array(entries);
    shown::set_first_turn(&mut returned, turns);
    Draft {
        tool: TURNS_TOOL,
        query: query.to_owned(),
        kind_filter: None,
        returned,
        rerank_state: report.state.as_str(),
    }
}

fn scored_entry(reference: Value, score: Value, rerank_score: Option<&Value>) -> Value {
    let mut entry = json!({"reference": reference, "score": score});
    if let Some(rerank_score) = rerank_score {
        entry["rerank_score"] = rerank_score.clone();
    }
    entry
}

fn state_label(state: Option<&str>) -> &'static str {
    match state {
        Some("reranked") => "reranked",
        Some("loading") => "loading",
        Some("unavailable") => "unavailable",
        _ => "off",
    }
}

pub(super) fn spawn_record(state: &AppState, session: Option<&str>, draft: Draft) {
    let Some(persist) = state.persist().cloned() else {
        return;
    };
    let session = session.and_then(|raw| Uuid::parse_str(raw.trim()).ok());
    state.tracker.spawn(async move {
        let event = NewSearchEvent {
            session,
            tool: draft.tool,
            query: &draft.query,
            kind_filter: draft.kind_filter.as_deref(),
            returned: &draft.returned,
            rerank_state: draft.rerank_state,
        };
        if let Err(error) = persist.record_search_event(&event).await {
            tracing::warn!(tool = draft.tool, error = %error, "search_events: could not record a search");
        }
    });
}
