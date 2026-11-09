use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::store::persistence::work_threads::cards::{CardInputs, CardThread};

mod build;

#[cfg(test)]
#[path = "cards_tests.rs"]
mod cards_tests;

pub(crate) const MAX_CARD_BYTES: usize = 1500;
const HEADER_FIELD_CHARS: usize = 80;
const MIN_SHRINK_CHARS: usize = 24;
const PROTECTED_PRIORITY: u8 = 2;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RenderedCard {
    pub(crate) card: Value,
    pub(crate) text: String,
    pub(crate) content_hash: String,
    pub(crate) refs: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SectionKind {
    WhereWeAre,
    NextUp,
    Blockers,
    OpenQuestions,
    LastHandoff,
    DecisionsSince,
    OpenPromises,
    UnmetAsks,
}

impl SectionKind {
    fn key(self) -> &'static str {
        match self {
            Self::WhereWeAre => "where_we_are",
            Self::NextUp => "next_up",
            Self::Blockers => "blockers",
            Self::OpenQuestions => "open_questions",
            Self::LastHandoff => "last_handoff",
            Self::DecisionsSince => "decisions_since",
            Self::OpenPromises => "open_promises",
            Self::UnmetAsks => "unmet_asks",
        }
    }

    fn heading(self) -> &'static str {
        match self {
            Self::WhereWeAre => "Where we are",
            Self::NextUp => "Next up",
            Self::Blockers => "Blockers",
            Self::OpenQuestions => "Open questions",
            Self::LastHandoff => "Last handoff",
            Self::DecisionsSince => "Decisions since",
            Self::OpenPromises => "Open promises",
            Self::UnmetAsks => "Unmet asks",
        }
    }

    fn drop_priority(self) -> u8 {
        match self {
            Self::WhereWeAre => 0,
            Self::NextUp => 1,
            Self::Blockers => 2,
            Self::LastHandoff => 3,
            Self::OpenQuestions => 4,
            Self::DecisionsSince => 5,
            Self::OpenPromises => 6,
            Self::UnmetAsks => 7,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct Item {
    pub(super) body: String,
    pub(super) refs: Vec<String>,
}

impl Item {
    fn line(&self) -> String {
        format!("{} [{}]", self.body, self.refs.join(" "))
    }
}

#[derive(Debug, Clone)]
pub(super) struct Section {
    pub(super) kind: SectionKind,
    pub(super) items: Vec<Item>,
}

pub(super) fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn clip(text: &str, max_chars: usize) -> String {
    let squashed = squash(text);
    if squashed.chars().count() <= max_chars {
        return squashed;
    }
    ellipsize(&squashed, max_chars)
}

fn ellipsize(text: &str, max_chars: usize) -> String {
    let kept: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

fn header(thread: &CardThread) -> (String, Vec<String>) {
    let status = thread.status.as_deref().unwrap_or("unknown");
    let mut line = format!("Work thread `{}` ({status})", thread.slug);
    if let Some(branch) = thread.branch.as_deref().filter(|b| !b.trim().is_empty()) {
        line.push_str(&format!(", branch `{}`", clip(branch, HEADER_FIELD_CHARS)));
    }
    if let Some(title) = thread.title.as_deref().filter(|t| !t.trim().is_empty()) {
        line.push_str(&format!(": {}", clip(title, HEADER_FIELD_CHARS)));
    }
    let token = format!("thread:{}", thread.slug);
    line.push_str(&format!(" [{token}]"));
    (line, vec![token])
}

fn render_section(section: &Section) -> String {
    let heading = section.kind.heading();
    match section.items.as_slice() {
        [only] => format!("{heading}: {}", only.line()),
        items => {
            let bullets: Vec<String> = items
                .iter()
                .map(|item| format!("- {}", item.line()))
                .collect();
            format!("{heading}:\n{}", bullets.join("\n"))
        }
    }
}

fn render_text(header_line: &str, sections: &[Section]) -> String {
    let mut parts = vec![header_line.to_owned()];
    parts.extend(sections.iter().map(render_section));
    parts.join("\n")
}

fn drop_lowest(sections: &mut Vec<Section>, protected_floor: Option<u8>) -> bool {
    let target = sections
        .iter()
        .enumerate()
        .filter(|(_, section)| {
            protected_floor.is_none_or(|floor| section.kind.drop_priority() > floor)
        })
        .max_by_key(|(_, section)| section.kind.drop_priority())
        .map(|(index, _)| index);
    let Some(index) = target else {
        return false;
    };
    sections[index].items.pop();
    sections.retain(|section| !section.items.is_empty());
    true
}

fn shrink_longest(sections: &mut [Section]) -> bool {
    let longest = sections
        .iter_mut()
        .flat_map(|section| section.items.iter_mut())
        .max_by_key(|item| item.body.chars().count());
    let Some(item) = longest else {
        return false;
    };
    let chars = item.body.chars().count();
    if chars <= MIN_SHRINK_CHARS {
        return false;
    }
    let keep = (chars / 2).max(MIN_SHRINK_CHARS);
    item.body = ellipsize(&item.body, keep);
    true
}

fn reduce(sections: &mut Vec<Section>) -> bool {
    drop_lowest(sections, Some(PROTECTED_PRIORITY))
        || shrink_longest(sections)
        || drop_lowest(sections, None)
}

fn fit(header_line: &str, sections: &mut Vec<Section>) -> String {
    loop {
        let text = render_text(header_line, sections);
        if text.len() <= MAX_CARD_BYTES || !reduce(sections) {
            return text;
        }
    }
}

fn collect_refs(header_refs: Vec<String>, sections: &[Section]) -> Vec<String> {
    let mut refs = header_refs;
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

fn card_json(thread: &CardThread, sections: &[Section]) -> Value {
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

pub(crate) fn render(inputs: &CardInputs) -> RenderedCard {
    let (header_line, header_refs) = header(&inputs.thread);
    let mut sections = build::sections(inputs);
    let text = fit(&header_line, &mut sections);
    let content_hash = hex::encode(Sha256::digest(text.as_bytes()));
    let refs = collect_refs(header_refs, &sections);
    RenderedCard {
        card: card_json(&inputs.thread, &sections),
        text,
        content_hash,
        refs: json!(refs),
    }
}
