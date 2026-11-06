use std::sync::LazyLock;

use regex::Regex;
use serde_json::json;

use crate::store::persistence::dossier_entries::{
    DIRECTIVES_VERSION, DirectiveSourceRow, EntryKind, NewDossierEntry, SourceRef,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Polarity {
    Allow,
    Forbid,
}

impl Polarity {
    pub fn code(self) -> &'static str {
        match self {
            Polarity::Allow => "allow",
            Polarity::Forbid => "forbid",
        }
    }
}

pub struct DirectivePattern {
    pub class: &'static str,
    pub polarity: Polarity,
    pub pattern: String,
    regex: Regex,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectiveMatch {
    pub class: &'static str,
    pub polarity: Polarity,
    pub pattern: String,
    pub span: (usize, usize),
}

const NEG: &str = "(don['’]t|do not|never|no need to)";
const GRANT: &str = "(go ahead and|you can|feel free to|please|then)";
const LATER: &str = "(it|this|when|once|after|if)";

fn verb_patterns(verb: &str, gerund: &str) -> Vec<(Polarity, String)> {
    vec![
        (Polarity::Allow, format!(r"\b{GRANT} {verb}\b")),
        (Polarity::Allow, format!(r"\b{verb} {LATER}\b")),
        (Polarity::Forbid, format!(r"\b{NEG} {verb}\b")),
        (Polarity::Forbid, format!(r"\bwithout {gerund}\b")),
    ]
}

fn class_table() -> Vec<(&'static str, Polarity, String)> {
    let mut table: Vec<(&'static str, Polarity, String)> = Vec::new();
    for (class, verb, gerund) in [
        ("commit", "commit", "committing"),
        ("push", "push", "pushing"),
        ("merge", "merge", "merging"),
        ("deploy", "deploy", "deploying"),
    ] {
        for (polarity, pattern) in verb_patterns(verb, gerund) {
            table.push((class, polarity, pattern));
        }
    }
    table.push((
        "push",
        Polarity::Allow,
        r"\bpush when (green|ready|tests pass)\b".to_owned(),
    ));
    table.push((
        "pr",
        Polarity::Allow,
        r"\b(open|raise|create) (a )?(pr|pull request)\b".to_owned(),
    ));
    table.push((
        "pr",
        Polarity::Forbid,
        r"\b(don['’]t|do not) (open|raise|create) (a )?(pr|pull request)\b".to_owned(),
    ));
    for pattern in [
        r"\bjust do it\b",
        r"\b(don['’]t|do not|stop) ask(ing)?\b",
        r"\bno need to ask\b",
        r"\byou don['’]t need (my )?(permission|approval)\b",
        r"\bwithout asking\b",
    ] {
        table.push(("proceed", Polarity::Allow, pattern.to_owned()));
    }
    for pattern in [
        r"\bask (me )?(before|first)\b",
        r"\bcheck with me (before|first)\b",
        r"\bwait for my (go|ok|approval)\b",
    ] {
        table.push(("ask_first", Polarity::Forbid, pattern.to_owned()));
    }
    table
}

pub static PATTERNS: LazyLock<Vec<DirectivePattern>> = LazyLock::new(|| {
    class_table()
        .into_iter()
        .filter_map(|(class, polarity, pattern)| {
            let regex = Regex::new(&format!("(?i){pattern}")).ok()?;
            Some(DirectivePattern {
                class,
                polarity,
                pattern,
                regex,
            })
        })
        .collect()
});

fn char_offset(text: &str, byte: usize) -> usize {
    text[..byte].chars().count()
}

fn overlaps(a: (usize, usize), b: (usize, usize)) -> bool {
    a.0 < b.1 && b.0 < a.1
}

pub fn extract(text: &str) -> Vec<DirectiveMatch> {
    let mut found: Vec<DirectiveMatch> = Vec::new();
    for pattern in PATTERNS.iter() {
        for hit in pattern.regex.find_iter(text) {
            found.push(DirectiveMatch {
                class: pattern.class,
                polarity: pattern.polarity,
                pattern: pattern.pattern.clone(),
                span: (char_offset(text, hit.start()), char_offset(text, hit.end())),
            });
        }
    }
    let forbids: Vec<(&'static str, (usize, usize))> = found
        .iter()
        .filter(|m| m.polarity == Polarity::Forbid)
        .map(|m| (m.class, m.span))
        .collect();
    found.retain(|m| {
        m.polarity == Polarity::Forbid
            || !forbids
                .iter()
                .any(|(class, span)| *class == m.class && overlaps(*span, m.span))
    });
    found.sort_by_key(|m| (m.span.0, m.span.1));
    found
}

pub fn entries_for(sources: &[DirectiveSourceRow]) -> Vec<NewDossierEntry> {
    let mut entries: Vec<NewDossierEntry> = Vec::new();
    for source in sources {
        let mut seen: Vec<(&'static str, Polarity)> = Vec::new();
        for found in extract(&source.user_prompt) {
            if seen.contains(&(found.class, found.polarity)) {
                continue;
            }
            seen.push((found.class, found.polarity));
            entries.push(NewDossierEntry {
                session_id: source.session_id,
                operator_turn_id: Some(source.turn_id),
                kind: EntryKind::Directive.code(),
                data: json!({
                    "class": found.class,
                    "polarity": found.polarity.code(),
                    "pattern": found.pattern,
                    "span": [found.span.0, found.span.1],
                }),
                source_ref: SourceRef::request(source.start_request_id).to_string(),
                dedup_key: format!(
                    "request:{}:{}:{}",
                    source.start_request_id,
                    found.class,
                    found.polarity.code()
                ),
                extractor_version: DIRECTIVES_VERSION,
                effective_at: source.created_at,
            });
        }
    }
    entries
}
