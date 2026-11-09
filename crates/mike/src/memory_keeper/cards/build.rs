use super::MAX_CARD_BYTES;
use super::fit::{Item, ellipsize, squash};
use crate::store::persistence::work_threads::cards::{
    CardAsk, CardEntry, CardInputs, CardNote, CardPromise, CardRelated,
};

const SLUG_CHARS: usize = 120;
const SESSION_PREFIX_CHARS: usize = 8;
const RELATED_TEXT_BYTES: usize = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SectionKind {
    Header,
    Goal,
    WhereWeAre,
    NextUp,
    Blockers,
    OpenQuestions,
    DecisionsInForce,
    LastHandoff,
    DecisionsSince,
    OpenPromises,
    UnmetAsks,
    Related,
    Footer,
}

const ORDER: [SectionKind; 13] = [
    SectionKind::Header,
    SectionKind::Goal,
    SectionKind::WhereWeAre,
    SectionKind::NextUp,
    SectionKind::Blockers,
    SectionKind::OpenQuestions,
    SectionKind::DecisionsInForce,
    SectionKind::LastHandoff,
    SectionKind::DecisionsSince,
    SectionKind::OpenPromises,
    SectionKind::UnmetAsks,
    SectionKind::Related,
    SectionKind::Footer,
];

impl SectionKind {
    pub(crate) fn budget_through(self) -> usize {
        ORDER
            .iter()
            .take_while(|kind| **kind != self)
            .chain(std::iter::once(&self))
            .map(|kind| kind.budget())
            .sum()
    }

    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Header => "header",
            Self::Goal => "goal",
            Self::WhereWeAre => "where_we_are",
            Self::NextUp => "next_up",
            Self::Blockers => "blockers",
            Self::OpenQuestions => "open_questions",
            Self::DecisionsInForce => "decisions_in_force",
            Self::LastHandoff => "last_handoff",
            Self::DecisionsSince => "decisions_since",
            Self::OpenPromises => "open_promises",
            Self::UnmetAsks => "unmet_asks",
            Self::Related => "related",
            Self::Footer => "footer",
        }
    }

    pub(crate) fn heading(self) -> Option<&'static str> {
        match self {
            Self::Header | Self::Footer => None,
            Self::Goal => Some("Goal"),
            Self::WhereWeAre => Some("Where we are"),
            Self::NextUp => Some("Next up"),
            Self::Blockers => Some("Blockers"),
            Self::OpenQuestions => Some("Open questions"),
            Self::DecisionsInForce => Some("Decisions in force"),
            Self::LastHandoff => Some("Last handoff"),
            Self::DecisionsSince => Some("Decisions since"),
            Self::OpenPromises => Some("Open promises"),
            Self::UnmetAsks => Some("Unmet asks"),
            Self::Related => Some("Related"),
        }
    }

    pub(crate) fn allowance(self, used: usize) -> usize {
        let through = self.budget_through().min(MAX_CARD_BYTES);
        let room = match self {
            Self::Related => through
                .min(MAX_CARD_BYTES.saturating_sub(Self::Footer.budget()))
                .min(used + self.budget()),
            _ => through,
        };
        room.saturating_sub(used)
    }

    fn budget(self) -> usize {
        match self {
            Self::Header => 250,
            Self::Goal | Self::DecisionsInForce | Self::DecisionsSince => 110,
            Self::WhereWeAre => 190,
            Self::NextUp | Self::Footer => 150,
            Self::Blockers => 90,
            Self::Related => 120,
            Self::OpenQuestions | Self::OpenPromises | Self::UnmetAsks => 70,
            Self::LastHandoff => 130,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Section {
    pub(crate) kind: SectionKind,
    pub(crate) items: Vec<Item>,
}

pub(crate) fn card_slug(slug: &str) -> String {
    slug.chars().take(SLUG_CHARS).collect()
}

fn single(text: &str, token: &str) -> Vec<Item> {
    let body = squash(text);
    if body.is_empty() {
        return Vec::new();
    }
    vec![Item::new(body, vec![token.to_owned()])]
}

fn joined_lines(text: &str, token: &str) -> Vec<Item> {
    let lines: Vec<String> = text
        .lines()
        .map(|line| squash(line.trim().trim_start_matches(['-', '*'])))
        .filter(|line| !line.is_empty())
        .collect();
    single(&lines.join("; "), token)
}

fn header(inputs: &CardInputs, token: &str) -> Vec<Item> {
    let thread = &inputs.thread;
    let mut body = format!(
        "Work thread ({})",
        thread.status.as_deref().unwrap_or("unknown")
    );
    if let Some(branch) = thread
        .branch
        .as_deref()
        .map(squash)
        .filter(|b| !b.is_empty())
    {
        body.push_str(&format!(", branch `{branch}`"));
    }
    if let Some(title) = thread
        .title
        .as_deref()
        .map(squash)
        .filter(|t| !t.is_empty())
    {
        body.push_str(&format!(": {title}"));
    }
    vec![Item::new(body, vec![token.to_owned()])]
}

fn session_prefix(entry: &CardEntry) -> Option<String> {
    entry.session_uuid.map(|session| {
        session
            .simple()
            .to_string()
            .chars()
            .take(SESSION_PREFIX_CHARS)
            .collect()
    })
}

fn handoff(entry: &CardEntry) -> Item {
    let date = entry.entry_date.format("%Y-%m-%d");
    let summary = squash(&entry.summary);
    let body = match session_prefix(entry) {
        Some(prefix) => format!("{date} by session {prefix}: {summary}"),
        None => format!("{date}: {summary}"),
    };
    Item::new(body, vec![format!("entry:{}", entry.id)])
}

fn decision_entry(entry: &CardEntry) -> Item {
    Item::new(
        format!(
            "{}: {}",
            entry.entry_date.format("%Y-%m-%d"),
            squash(&entry.summary)
        ),
        vec![format!("entry:{}", entry.id)],
    )
}

fn decision_note(note: &CardNote) -> Item {
    Item::new(
        format!(
            "{} {}: {}",
            note.kind,
            note.created_at.format("%Y-%m-%d"),
            squash(&note.title)
        ),
        vec![format!("note:{}", note.note_uuid)],
    )
}

fn promise(promise: &CardPromise) -> Item {
    let text = promise.text.as_deref().map(squash).unwrap_or_default();
    let mut refs = vec![format!("promise:{}", promise.id)];
    refs.extend(promise.operator_turn_id.map(|turn| format!("turn:{turn}")));
    let date = promise.effective_at.format("%Y-%m-%d");
    let body = if text.is_empty() {
        format!("{date}: promise made at the end of a turn")
    } else {
        format!("{date}: {text}")
    };
    Item::new(body, refs)
}

fn ask(ask: &CardAsk) -> Item {
    Item::new(
        format!(
            "{}: {}",
            ask.created_at.format("%Y-%m-%d"),
            squash(&ask.unmet_ask)
        ),
        vec![format!("turn:{}", ask.operator_turn_id)],
    )
}

fn related(item: &CardRelated) -> Option<Item> {
    let body = ellipsize(&squash(&item.text), RELATED_TEXT_BYTES);
    if body.is_empty() {
        return None;
    }
    Some(Item::new(body, vec![item.reference.clone()]))
}

fn related_items(inputs: &CardInputs) -> Vec<Item> {
    let mut ordered: Vec<&CardRelated> = inputs.related.iter().collect();
    ordered.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.reference.cmp(&b.reference))
    });
    ordered.into_iter().filter_map(related).collect()
}

fn state_sections(inputs: &CardInputs, token: &str) -> Vec<(SectionKind, Vec<Item>)> {
    let Some(state) = inputs.state.as_ref() else {
        return Vec::new();
    };
    let where_we_are = if state.where_we_are.trim().is_empty() {
        &state.tldr
    } else {
        &state.where_we_are
    };
    vec![
        (SectionKind::WhereWeAre, single(where_we_are, token)),
        (SectionKind::NextUp, single(&state.next_up, token)),
        (SectionKind::Blockers, joined_lines(&state.blockers, token)),
        (
            SectionKind::OpenQuestions,
            joined_lines(&state.open_questions, token),
        ),
        (
            SectionKind::DecisionsInForce,
            joined_lines(&state.decisions, token),
        ),
    ]
}

pub(crate) fn sections(inputs: &CardInputs) -> Vec<Section> {
    let slug = card_slug(&inputs.thread.slug);
    let token = format!("thread:{slug}");
    let mut raw = vec![
        (SectionKind::Header, header(inputs, &token)),
        (
            SectionKind::Goal,
            single(inputs.thread.goal.as_deref().unwrap_or_default(), &token),
        ),
    ];
    raw.extend(state_sections(inputs, &token));
    raw.extend([
        (
            SectionKind::LastHandoff,
            inputs.handoff.iter().map(handoff).collect(),
        ),
        (
            SectionKind::DecisionsSince,
            inputs
                .decisions
                .iter()
                .map(decision_entry)
                .chain(inputs.notes.iter().map(decision_note))
                .collect(),
        ),
        (
            SectionKind::OpenPromises,
            inputs.promises.iter().map(promise).collect(),
        ),
        (
            SectionKind::UnmetAsks,
            inputs.unmet_asks.iter().map(ask).collect(),
        ),
        (SectionKind::Related, related_items(inputs)),
        (
            SectionKind::Footer,
            vec![Item::new(
                format!("Full state: work_thread_get {slug}"),
                Vec::new(),
            )],
        ),
    ]);
    raw.into_iter()
        .filter(|(_, items)| !items.is_empty())
        .map(|(kind, items)| Section { kind, items })
        .collect()
}
