use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::analysis::embeddings::{EmbeddingConfig, EmbeddingService};
use crate::dossier::view::AsOf;
use crate::semantic::config::{RelapseEvidence, SemanticConfig};
use crate::semantic::neighbours::{Neighbour, NeighbourSet, Retriever};
use crate::semantic::retrieve::{
    Audience, QuerySubject, RETRIEVAL_VERSION, RetrievedKind, Scope, SemanticQuery,
};
use crate::store::persistence::PersistHandle;

const K: usize = 3;

fn embedding_service(rows: &[(String, Value)]) -> EmbeddingService {
    let service = EmbeddingService::new(&EmbeddingConfig::default());
    for (key, value) in rows {
        service.apply_setting(key, value);
    }
    service
}

fn classify(neighbour: &Neighbour, anchor_turn_id: Option<i64>) -> &'static str {
    if anchor_turn_id.is_some() && neighbour.row.corrected_turn_id == anchor_turn_id {
        "in_g1_window"
    } else if neighbour.same_session {
        "older_in_session"
    } else {
        "other_session"
    }
}

fn annotated(set: &NeighbourSet, anchor_turn_id: Option<i64>) -> Value {
    let mut value = serde_json::to_value(set).unwrap_or(Value::Null);
    if let Some(items) = value.get_mut("neighbours").and_then(Value::as_array_mut) {
        for (item, neighbour) in items.iter_mut().zip(&set.neighbours) {
            if let Value::Object(map) = item {
                map.insert(
                    "classification".to_owned(),
                    Value::String(classify(neighbour, anchor_turn_id).to_owned()),
                );
            }
        }
    }
    value
}

fn status_only(status: &str) -> Value {
    serde_json::json!({
        "status": status,
        "retrieval_version": RETRIEVAL_VERSION,
        "embedder_key": Value::Null,
        "floor_ids": Vec::<i64>::new(),
        "neighbours": Vec::<Value>::new(),
    })
}

pub async fn record(
    persist: &PersistHandle,
    session_db_id: i64,
    prompt_text: &str,
    ending_text: &str,
    anchor_turn_id: Option<i64>,
    now: DateTime<Utc>,
) -> Option<Value> {
    let rows = match persist.fetch_settings().await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(error = %e, "turn guard: relapse evidence settings unavailable");
            return None;
        }
    };
    let semantic = SemanticConfig::from_rows(&rows);
    if matches!(semantic.relapse_evidence, RelapseEvidence::Off) {
        return None;
    }
    let embedding = embedding_service(&rows);
    let retriever = Retriever {
        persist,
        embedding: Some(&embedding),
        config: &semantic,
        live_consumer: true,
    };
    let query = SemanticQuery {
        subject: QuerySubject::OpenTurn {
            session_id: session_db_id,
            prompt_text: prompt_text.to_owned(),
            ending_text: ending_text.to_owned(),
        },
        audience: Audience::Steerer,
        kinds: vec![RetrievedKind::Correction],
        scope: Scope::Operator,
        as_of: AsOf::knowledge(now),
        k: K,
        offset: 0,
    };
    let timeout = Duration::from_millis(semantic.inline_timeout_ms);
    let value = match tokio::time::timeout(timeout, retriever.neighbours(&query)).await {
        Ok(Ok(set)) => annotated(&set, anchor_turn_id),
        Ok(Err(e)) => status_only(e.status_code()),
        Err(_) => status_only("embed_timeout"),
    };
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::persistence::semantic::CandidateRow;

    fn neighbour(item_id: i64, corrected_turn_id: Option<i64>, same_session: bool) -> Neighbour {
        Neighbour {
            kind: RetrievedKind::Correction,
            item_id,
            pair: "prompt:corr".to_owned(),
            similarity: 0.9,
            score: 0.1,
            floor_id: 1,
            same_session,
            row: CandidateRow {
                item_id,
                item_facet: "corr".to_owned(),
                text: "an earlier correction".to_owned(),
                source_ref: "memory.semantic_items:1".to_owned(),
                operator_turn_id: Some(item_id),
                session_id: Some(if same_session { 42 } else { 99 }),
                project_id: None,
                request_id: None,
                effective_at: Utc::now(),
                recorded_at: Utc::now(),
                query_facet: "prompt".to_owned(),
                similarity: 0.9,
                qualification_ref: None,
                qualification: None,
                corrected_turn_id,
            },
        }
    }

    #[test]
    fn classify_marks_g1_anchor_before_same_session() {
        let anchor = neighbour(1, Some(500), true);
        assert_eq!(classify(&anchor, Some(500)), "in_g1_window");
    }

    #[test]
    fn classify_marks_same_session_without_anchor_match() {
        let n = neighbour(2, Some(600), true);
        assert_eq!(classify(&n, Some(500)), "older_in_session");
    }

    #[test]
    fn classify_marks_other_session() {
        let n = neighbour(3, None, false);
        assert_eq!(classify(&n, None), "other_session");
    }

    #[test]
    fn annotated_adds_classification_per_neighbour() {
        let set = NeighbourSet {
            status: "ok",
            retrieval_version: RETRIEVAL_VERSION,
            embedder_key: Some("k1".to_owned()),
            audience: "steerer",
            scope: "operator",
            as_of: Utc::now(),
            as_of_mode: "knowledge",
            query_facets: vec!["prompt".to_owned()],
            floor_ids: vec![1],
            total: 1,
            neighbours: vec![neighbour(1, Some(500), true)],
        };
        let value = annotated(&set, Some(500));
        assert_eq!(value["neighbours"][0]["classification"], "in_g1_window");
        assert_eq!(value["status"], "ok");
    }

    #[test]
    fn status_only_names_empty_neighbours() {
        let value = status_only("egress_blocked");
        assert_eq!(value["status"], "egress_blocked");
        assert!(value["neighbours"].as_array().is_some_and(Vec::is_empty));
    }
}
