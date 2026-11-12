use serde_json::{Map, Value, json};

use crate::search_ranking::{identifier_hits, is_outside_project};

pub(crate) const ALSO_REFERENCES: usize = 3;
pub(crate) const LEAD_TITLE_CHARS: usize = 200;

const KEPT_ENVELOPE: [&str; 5] = ["project_uuid", "items", "more", "has_more", "next_offset"];
const KEPT_ITEM_FIELDS: [&str; 6] = [
    "reference",
    "source_kind",
    "title",
    "updated_at",
    "excerpt",
    "excerpt_from",
];

pub(super) fn lead_item(item: &Value) -> Value {
    let mut lead = Map::new();
    for field in KEPT_ITEM_FIELDS {
        if let Some(value) = item.get(field) {
            lead.insert(field.to_owned(), value.clone());
        }
    }
    if let Some(title) = item["title"].as_str() {
        let title = super::best_shape::fit(&super::best_shape::one_line(title), LEAD_TITLE_CHARS);
        lead.insert("title".to_owned(), json!(title));
    }
    if let Some(session) = session_uuid(item) {
        lead.insert("session_uuid".to_owned(), session);
    }
    if let Some(title) = session_title(item) {
        lead.insert("session_title".to_owned(), json!(title));
    }
    let hits = identifier_hits(item);
    if hits > 0 {
        lead.insert("identifier_hits".to_owned(), json!(hits));
    }
    also(item, &mut lead);
    if let Some(provenance) = lean_provenance(item) {
        lead.insert("provenance".to_owned(), provenance);
    }
    Value::Object(lead)
}

fn session_uuid(item: &Value) -> Option<Value> {
    [
        &item["author"]["session_uuid"],
        &item["provenance"]["session_uuid"],
    ]
    .into_iter()
    .find(|value| value.is_string())
    .cloned()
}

pub(super) fn session_title(item: &Value) -> Option<&str> {
    item["author"]["session_title"]
        .as_str()
        .map(str::trim)
        .filter(|title| !title.is_empty())
}

pub(super) const LATER_MARKS: [(&str, &str); 2] = [
    ("superseded_by", "superseded by"),
    ("contradicted_by", "contradicted by"),
];

pub(super) const RETIRED: &str = "retired";

pub(super) fn retired(item: &Value) -> bool {
    item["provenance"][RETIRED].is_object()
}

pub(super) fn later_mark(item: &Value) -> Option<(&'static str, &'static str, &str)> {
    LATER_MARKS.iter().find_map(|(field, words)| {
        item["provenance"][*field]["reference"]
            .as_str()
            .map(|reference| (*field, *words, reference))
    })
}

fn also(item: &Value, lead: &mut Map<String, Value>) {
    let count = item["also_count"].as_u64().unwrap_or(0);
    if count == 0 {
        return;
    }
    let references: Vec<Value> = item["also"]
        .as_array()
        .map(|also| also.iter().take(ALSO_REFERENCES).cloned().collect())
        .unwrap_or_default();
    lead.insert("also".to_owned(), Value::Array(references));
    lead.insert("also_count".to_owned(), json!(count));
}

fn lean_provenance(item: &Value) -> Option<Value> {
    let mut provenance = Map::new();
    if item["provenance"]["written_by"].as_str() == Some("machine") {
        provenance.insert("written_by".to_owned(), json!("machine"));
    }
    if is_outside_project(item) {
        provenance.insert("outside_project".to_owned(), json!(true));
    }
    for field in LATER_MARKS
        .map(|(field, _)| field)
        .into_iter()
        .chain([RETIRED])
    {
        let mark = &item["provenance"][field];
        if mark.is_object() {
            provenance.insert(field.to_owned(), mark.clone());
        }
    }
    (!provenance.is_empty()).then_some(Value::Object(provenance))
}

pub(super) fn lean_envelope(answer: &mut Value) {
    let Value::Object(source) = std::mem::take(answer) else {
        return;
    };
    let mut lean = Map::new();
    for key in KEPT_ENVELOPE {
        if let Some(value) = source.get(key) {
            lean.insert(key.to_owned(), value.clone());
        }
    }
    if let Some(rerank) = source.get("rerank").and_then(lean_rerank) {
        lean.insert("rerank".to_owned(), rerank);
    }
    if let Some(fallback) = source
        .get("embedding")
        .map(|embedding| &embedding["fallback"])
        .filter(|fallback| !fallback.is_null())
    {
        lean.insert("embedding".to_owned(), json!({"fallback": fallback}));
    }
    *answer = Value::Object(lean);
}

fn lean_rerank(rerank: &Value) -> Option<Value> {
    let mut lean = Map::new();
    lean.insert("state".to_owned(), rerank.get("state")?.clone());
    if let Some(detail) = rerank.get("detail").filter(|detail| !detail.is_null()) {
        lean.insert("detail".to_owned(), detail.clone());
    }
    if let Some(left_out) = rerank
        .get("left_out")
        .filter(|left_out| !left_out.is_null())
    {
        lean.insert("left_out".to_owned(), left_out.clone());
    }
    if rerank["ready_in_s"].as_u64().is_some_and(|ready| ready > 0) {
        lean.insert("ready_in_s".to_owned(), rerank["ready_in_s"].clone());
    }
    Some(Value::Object(lean))
}
