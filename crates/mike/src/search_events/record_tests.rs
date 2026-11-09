use serde_json::json;

use super::record::knowledge_draft;
use crate::store::persistence::project_knowledge::KnowledgeQuery;

fn input(kind: Option<&str>) -> KnowledgeQuery {
    KnowledgeQuery {
        project: "p".into(),
        query: "cache rules".into(),
        kind: kind.map(str::to_owned),
        ..Default::default()
    }
}

#[test]
fn knowledge_draft_from_a_reranked_answer_keeps_order_scores_and_state() {
    let answer = json!({
        "items": [
            {"reference": "lesson:b", "rank": 0.01, "rerank_score": 0.93},
            {"reference": "artifact:4", "rank": 0.02, "rerank_score": 0.41},
            {"reference": "turn:9", "rank": 0.005}
        ],
        "rerank": {"state": "reranked", "ready_in_s": 0}
    });

    let draft = knowledge_draft(&input(Some("lesson")), &answer);

    assert_eq!(draft.tool, "search_project_knowledge");
    assert_eq!(draft.query, "cache rules");
    assert_eq!(draft.kind_filter.as_deref(), Some("lesson"));
    assert_eq!(draft.rerank_state, "reranked");
    assert_eq!(
        draft.returned,
        json!([
            {"reference": "lesson:b", "score": 0.01, "rerank_score": 0.93},
            {"reference": "artifact:4", "score": 0.02, "rerank_score": 0.41},
            {"reference": "turn:9", "score": 0.005}
        ])
    );
}

#[test]
fn knowledge_draft_from_an_answer_without_a_rerank_object_is_off() {
    let answer = json!({"items": [{"reference": "lesson:b", "rank": 0.01}]});

    let draft = knowledge_draft(&input(None), &answer);

    assert_eq!(draft.rerank_state, "off");
    assert_eq!(draft.kind_filter, None);
    assert_eq!(draft.returned.as_array().map(Vec::len), Some(1));
}

#[test]
fn knowledge_draft_from_an_answer_without_items_has_an_empty_return() {
    let draft = knowledge_draft(&input(None), &json!({"rerank": {"state": "loading"}}));

    assert_eq!(draft.rerank_state, "loading");
    assert_eq!(draft.returned, json!([]));
}
