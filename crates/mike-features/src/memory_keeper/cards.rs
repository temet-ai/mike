use serde_json::{Value, json};

use crate::store::persistence::work_threads::cards::{CardInputs, DirtyThread, NewCard};
use crate::wire::manifest::sha256_hex;

pub mod build;
pub mod fit;

use build::{Section, sections};

#[cfg(test)]
#[path = "cards_tests.rs"]
pub mod cards_tests;

pub const MAX_CARD_BYTES: usize = 1500;

#[derive(Debug, Clone, PartialEq)]
pub struct RenderedCard {
    pub card: Value,
    pub text: String,
    pub content_hash: String,
    pub refs: Value,
}

impl RenderedCard {
    pub fn into_new_card(self, dirty: DirtyThread) -> NewCard {
        NewCard {
            thread_id: dirty.thread_id,
            card: self.card,
            text: self.text,
            content_hash: self.content_hash,
            refs: self.refs,
            source_max_at: dirty.observed_at,
        }
    }
}

fn fitted(raw: Vec<Section>) -> Vec<Section> {
    let mut used = 0;
    let mut kept = Vec::new();
    for section in raw {
        let kind = section.kind;
        let allowance = kind.allowance(used);
        if let Some(items) = fit::fit(kind.heading(), section.items, allowance) {
            used += fit::render(kind.heading(), &items).len() + 1;
            kept.push(Section { kind, items });
        }
    }
    kept
}

fn render_text(sections: &[Section]) -> String {
    sections
        .iter()
        .map(|section| fit::render(section.kind.heading(), &section.items))
        .collect::<Vec<_>>()
        .join("\n")
}

fn collect_refs(sections: &[Section]) -> Vec<String> {
    let mut refs: Vec<String> = Vec::new();
    for token in sections
        .iter()
        .flat_map(|section| section.items.iter())
        .flat_map(|item| item.refs.iter())
    {
        if !refs.contains(token) {
            refs.push(token.clone());
        }
    }
    refs
}

fn card_json(inputs: &CardInputs, sections: &[Section]) -> Value {
    let rendered: Vec<Value> = sections
        .iter()
        .map(|section| {
            let items: Vec<Value> = section
                .items
                .iter()
                .map(|item| json!({ "text": item.body, "refs": item.refs }))
                .collect();
            json!({ "section": section.kind.key(), "items": items })
        })
        .collect();
    let thread = &inputs.thread;
    json!({
        "thread": {
            "slug": thread.slug,
            "status": thread.status,
            "branch": thread.branch,
            "worktree": thread.worktree,
            "title": thread.title,
            "goal": thread.goal,
            "last_session_uuid": thread.last_session_uuid,
        },
        "sections": rendered,
    })
}

pub fn render(inputs: &CardInputs) -> RenderedCard {
    let sections = fitted(sections(inputs));
    let text = render_text(&sections);
    RenderedCard {
        card: card_json(inputs, &sections),
        content_hash: sha256_hex(text.as_bytes()),
        refs: json!(collect_refs(&sections)),
        text,
    }
}
