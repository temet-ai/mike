use crate::store::persistence::PersistHandle;
use crate::store::persistence::consolidation::{DecisionRow, StatementDetail};
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::HashMap;

const OPERATOR_KINDS: [&str; 3] = ["correction", "unmet_ask", "question_answer"];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CarrierDto {
    pub reference: String,
    pub kind: &'static str,
    pub label: String,
    pub lesson_id: Option<i64>,
    pub source_name: Option<String>,
    pub relative_path: Option<String>,
    pub scope: &'static str,
    pub stated_at: Option<DateTime<Utc>>,
}

pub fn carrier_of(detail: &StatementDetail) -> CarrierDto {
    let lesson = detail.kind == "lesson";
    let label = if lesson {
        format!("Lesson {}", detail.source_id)
    } else {
        match (&detail.source_name, &detail.relative_path) {
            (Some(source), Some(path)) => format!("{source} · {path}"),
            (Some(source), None) => source.clone(),
            _ => detail.carrier_reference.clone(),
        }
    };
    CarrierDto {
        reference: detail.carrier_reference.clone(),
        kind: if lesson { "lesson" } else { "file" },
        label,
        lesson_id: lesson.then_some(detail.source_id),
        source_name: detail.source_name.clone(),
        relative_path: detail.relative_path.clone(),
        scope: if detail.project_id.is_some() { "project" } else { "machine" },
        stated_at: Some(detail.stated_at),
    }
}

pub fn carrier_from_reference(reference: &str) -> CarrierDto {
    let lesson = reference.starts_with("lesson:");
    CarrierDto {
        reference: reference.to_owned(),
        kind: if lesson { "lesson" } else { "file" },
        label: reference.to_owned(),
        lesson_id: reference
            .strip_prefix("lesson:")
            .and_then(|rest| rest.parse().ok()),
        source_name: None,
        relative_path: None,
        scope: "project",
        stated_at: None,
    }
}

pub fn evidence_list(raw: &Value) -> Vec<Value> {
    raw.as_array()
        .into_iter()
        .flatten()
        .filter(|item| item.get("file_edit").is_none())
        .map(|item| {
            let text = |key: &str| item.get(key).and_then(Value::as_str).unwrap_or_default();
            let kind = text("kind");
            let note = [text("shows"), text("excerpt")]
                .into_iter()
                .find(|t| !t.trim().is_empty())
                .unwrap_or_default();
            json!({
                "label": if text("reference").is_empty() { kind } else { text("reference") },
                "note": note,
                "at": item.get("dated_at").cloned().unwrap_or(Value::Null),
                "operator": OPERATOR_KINDS.contains(&kind),
            })
        })
        .collect()
}

pub async fn details_by_id(
    persist: &PersistHandle,
    ids: &[i64],
) -> Result<HashMap<i64, StatementDetail>, String> {
    Ok(persist
        .knowledge()
        .statement_details(ids)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|d| (d.statement_id, d))
        .collect())
}

pub async fn decisions_json(
    persist: &PersistHandle,
    rows: &[DecisionRow],
) -> Result<Vec<Value>, String> {
    let ids: Vec<i64> = rows
        .iter()
        .flat_map(|r| [r.statement_id, r.counterpart_statement_id])
        .flatten()
        .collect();
    let details = details_by_id(persist, &ids).await?;
    rows.iter()
        .map(|row| {
            let carrier = row
                .statement_id
                .and_then(|id| details.get(&id))
                .map_or_else(|| carrier_from_reference(&row.carrier_reference), carrier_of);
            let counterpart = row.counterpart_statement_id.and_then(|id| details.get(&id));
            let counterpart_dto = match (counterpart, &row.counterpart_reference) {
                (Some(detail), _) => Some(carrier_of(detail)),
                (None, Some(reference)) => Some(carrier_from_reference(reference)),
                (None, None) => None,
            };
            let mut value = serde_json::to_value(row).map_err(|e| e.to_string())?;
            if let Value::Object(map) = &mut value {
                map.insert("carrier".to_owned(), json!(carrier));
                map.insert("counterpart".to_owned(), json!(counterpart_dto));
                map.insert(
                    "counterpart_text".to_owned(),
                    json!(counterpart.map(|d| d.text.clone())),
                );
                map.insert("evidence".to_owned(), Value::Array(evidence_list(&row.evidence)));
            }
            Ok(value)
        })
        .collect()
}

pub async fn statements_json(persist: &PersistHandle, ids: &[i64]) -> Result<Vec<Value>, String> {
    let details = details_by_id(persist, ids).await?;
    Ok(ids
        .iter()
        .filter_map(|id| details.get(id))
        .map(|d| {
            json!({
                "statement_id": d.statement_id,
                "text": d.text,
                "carrier": carrier_of(d),
                "state": d.state,
                "resolved_by_statement_id": d.resolved_by_statement_id,
                "stated_at": d.stated_at,
            })
        })
        .collect())
}
