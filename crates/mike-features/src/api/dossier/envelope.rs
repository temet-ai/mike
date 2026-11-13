use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::dossier::render::{RENDERER_VERSION, RenderedDossier};
use crate::dossier::view::DossierView;
use crate::dossier::view::digest::{inputs_digest, source_versions};
use crate::dossier::view::types::DossierInputs;

#[derive(Debug, Clone, Copy)]
pub struct Paging {
    pub offset: usize,
    pub limit: usize,
}

pub fn page<T: Serialize>(items: &[T], paging: Paging) -> Value {
    let total = items.len();
    let end = paging.offset.saturating_add(paging.limit).min(total);
    let start = paging.offset.min(total);
    let next_offset = (end < total).then_some(end);
    json!({
        "total": total,
        "offset": paging.offset,
        "limit": paging.limit,
        "next_offset": next_offset,
        "items": items[start..end].iter().map(|i| serde_json::to_value(i).unwrap_or(Value::Null)).collect::<Vec<_>>(),
    })
}

pub fn view_value(view: &DossierView, paging: Paging, only: Option<&str>) -> Value {
    let mut out = Map::new();
    let mut put = |name: &str, value: Value| {
        if only.is_none_or(|o| o == name) {
            out.insert(name.to_owned(), value);
        }
    };
    put(
        "session",
        serde_json::to_value(&view.session).unwrap_or(Value::Null),
    );
    put("stream", page(&view.stream, paging));
    put("turns", page(&view.turns, paging));
    put("asks", page(&view.asks, paging));
    put("directives", page(&view.directives, paging));
    put("promises", page(&view.promises, paging));
    put(
        "open_turn",
        serde_json::to_value(&view.open_turn).unwrap_or(Value::Null),
    );
    put("corrections", page(&view.corrections, paging));
    put("reactions", page(&view.reactions, paging));
    put(
        "exclusions",
        serde_json::to_value(&view.exclusions).unwrap_or(Value::Null),
    );
    Value::Object(out)
}

pub fn envelope(
    inputs: &DossierInputs,
    view: &DossierView,
    rendered: Option<&RenderedDossier>,
    paging: Paging,
    only: Option<&str>,
) -> Value {
    json!({
        "session_uuid": inputs.session.session_uuid,
        "session_db_id": inputs.session.session_db_id,
        "as_of": inputs.as_of.at,
        "mode": inputs.as_of.mode,
        "renderer_version": RENDERER_VERSION,
        "source_versions": source_versions(inputs),
        "inputs_digest": inputs_digest(inputs),
        "view": view_value(view, paging, only),
        "rendered": rendered.map(|r| json!({
            "audience": r.audience,
            "budget_chars": r.budget_chars,
            "text": r.text,
            "sections": r.sections,
            "typed": r.typed,
            "digest_version": r.digest_version(),
        })),
    })
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn page_returns_whole_items_with_a_next_offset() {
        let items: Vec<i32> = (0..7).collect();
        let value = page(
            &items,
            Paging {
                offset: 2,
                limit: 3,
            },
        );
        assert_eq!(value["items"], json!([2, 3, 4]));
        assert_eq!(value["total"], 7);
        assert_eq!(value["next_offset"], 5);
        let last = page(
            &items,
            Paging {
                offset: 6,
                limit: 3,
            },
        );
        assert_eq!(last["next_offset"], Value::Null);
    }
}
