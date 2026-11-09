use super::{Item, Section, SectionKind, clip, squash};
use crate::store::persistence::work_threads::cards::{
    CardAsk, CardEntry, CardInputs, CardNote, CardPromise,
};

const WHERE_CHARS: usize = 300;
const NEXT_CHARS: usize = 240;
const BLOCKER_CHARS: usize = 200;
const QUESTION_CHARS: usize = 160;
const HANDOFF_CHARS: usize = 240;
const DECISION_CHARS: usize = 140;
const PROMISE_CHARS: usize = 140;
const ASK_CHARS: usize = 140;
const FIELD_LINES: usize = 3;
const DECISION_ENTRIES: usize = 5;
const DECISION_NOTES: usize = 3;
const PROMISES: usize = 3;
const ASKS: usize = 3;
const SESSION_PREFIX_CHARS: usize = 8;

pub(super) fn sections(inputs: &CardInputs) -> Vec<Section> {
    let thread_ref = format!("thread:{}", inputs.thread.slug);
    let (where_we_are, next_up, blockers, questions) = match inputs.state.as_ref() {
        Some(state) => (
            if state.where_we_are.trim().is_empty() {
                state.tldr.as_str()
            } else {
                state.where_we_are.as_str()
            },
            state.next_up.as_str(),
            state.blockers.as_str(),
            state.open_questions.as_str(),
        ),
        None => ("", "", "", ""),
    };
    [
        (
            SectionKind::WhereWeAre,
            whole_item(where_we_are, WHERE_CHARS, &thread_ref),
        ),
        (
            SectionKind::NextUp,
            whole_item(next_up, NEXT_CHARS, &thread_ref),
        ),
        (
            SectionKind::Blockers,
            field_items(blockers, FIELD_LINES, BLOCKER_CHARS, &thread_ref),
        ),
        (
            SectionKind::OpenQuestions,
            field_items(questions, FIELD_LINES, QUESTION_CHARS, &thread_ref),
        ),
        (SectionKind::LastHandoff, handoff_items(inputs)),
        (SectionKind::DecisionsSince, decision_items(inputs)),
        (SectionKind::OpenPromises, promise_items(&inputs.promises)),
        (SectionKind::UnmetAsks, ask_items(&inputs.unmet_asks)),
    ]
    .into_iter()
    .filter(|(_, items)| !items.is_empty())
    .map(|(kind, items)| Section { kind, items })
    .collect()
}

fn whole_item(text: &str, max_chars: usize, token: &str) -> Vec<Item> {
    let body = clip(text, max_chars);
    if body.is_empty() {
        return Vec::new();
    }
    vec![Item {
        body,
        refs: vec![token.to_owned()],
    }]
}

fn field_items(text: &str, max_lines: usize, max_chars: usize, token: &str) -> Vec<Item> {
    text.lines()
        .map(|line| line.trim().trim_start_matches(['-', '*']).trim())
        .filter(|line| !line.is_empty())
        .take(max_lines)
        .map(|line| Item {
            body: clip(line, max_chars),
            refs: vec![token.to_owned()],
        })
        .collect()
}

fn handoff_items(inputs: &CardInputs) -> Vec<Item> {
    inputs
        .handoff
        .iter()
        .map(|entry| {
            let date = entry.entry_date.format("%Y-%m-%d");
            let summary = clip(&entry.summary, HANDOFF_CHARS);
            let body = match entry.session_uuid {
                Some(session) => {
                    let prefix: String = session
                        .simple()
                        .to_string()
                        .chars()
                        .take(SESSION_PREFIX_CHARS)
                        .collect();
                    format!("{date} by session {prefix}: {summary}")
                }
                None => format!("{date}: {summary}"),
            };
            Item {
                body,
                refs: vec![format!("entry:{}", entry.id)],
            }
        })
        .collect()
}

fn decision_entry(entry: &CardEntry) -> Item {
    Item {
        body: format!(
            "{}: {}",
            entry.entry_date.format("%Y-%m-%d"),
            clip(&entry.summary, DECISION_CHARS)
        ),
        refs: vec![format!("entry:{}", entry.id)],
    }
}

fn decision_note(note: &CardNote) -> Item {
    Item {
        body: format!(
            "{} {}: {}",
            note.kind,
            note.created_at.format("%Y-%m-%d"),
            clip(&note.title, DECISION_CHARS)
        ),
        refs: vec![format!("note:{}", note.note_uuid)],
    }
}

fn decision_items(inputs: &CardInputs) -> Vec<Item> {
    inputs
        .decisions
        .iter()
        .take(DECISION_ENTRIES)
        .map(decision_entry)
        .chain(inputs.notes.iter().take(DECISION_NOTES).map(decision_note))
        .collect()
}

fn tail(text: &str, max_chars: usize) -> String {
    let squashed = squash(text);
    let count = squashed.chars().count();
    if count <= max_chars {
        return squashed;
    }
    let kept: String = squashed.chars().skip(count - (max_chars - 1)).collect();
    format!("…{}", kept.trim_start())
}

fn promise_item(promise: &CardPromise) -> Item {
    let excerpt = tail(&promise.excerpt, PROMISE_CHARS);
    let mut refs = vec![format!("promise:{}", promise.id)];
    refs.extend(promise.operator_turn_id.map(|turn| format!("turn:{turn}")));
    Item {
        body: if excerpt.is_empty() {
            "promise made at the end of a turn".to_owned()
        } else {
            excerpt
        },
        refs,
    }
}

fn promise_items(promises: &[CardPromise]) -> Vec<Item> {
    promises.iter().take(PROMISES).map(promise_item).collect()
}

fn ask_items(asks: &[CardAsk]) -> Vec<Item> {
    asks.iter()
        .take(ASKS)
        .map(|ask| Item {
            body: clip(&ask.unmet_ask, ASK_CHARS),
            refs: vec![format!("turn:{}", ask.operator_turn_id)],
        })
        .collect()
}
