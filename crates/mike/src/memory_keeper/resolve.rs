use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use uuid::Uuid;

const MIN_SCORE: f64 = 0.3;
const LEAD_MARGIN: f64 = 0.15;
const MAX_SEVERAL: usize = 3;
const RECENT_TURN_LIMIT: usize = 3;

const EXPLICIT_MENTION_WEIGHT: f64 = 1.0;
const RECENT_MENTION_WEIGHT: f64 = 0.45;
const BRANCH_WEIGHT: f64 = 0.6;
const WORKTREE_WEIGHT: f64 = 0.5;
const LAST_SESSION_WEIGHT: f64 = 0.5;

const RECENCY_WEIGHT: f64 = 0.5;
const RECENCY_HALF_LIFE_DAYS: f64 = 3.0;
const RECENCY_FLOOR: f64 = 0.15;
const SECONDS_PER_DAY: f64 = 86_400.0;

const WORD_WEIGHT: f64 = 0.2;
const WORD_COUNT_CAP: usize = 3;
const WORD_OVERLAP_FLOOR: usize = 2;
const MIN_WORD_CHARS: usize = 3;

const MIN_TICKET_PREFIX_CHARS: usize = 2;

const DEICTIC_PHRASES: &[&str] = &[
    "continue",
    "continuing",
    "carry on",
    "carrying on",
    "where are we",
    "where were we",
    "where we are",
    "where we left",
    "left off",
    "what's the status",
    "whats the status",
    "what is the status",
    "status update",
    "pick up",
    "pick this up",
    "pick it up",
    "picking up",
    "resume",
    "resuming",
    "the open epic",
    "the open thread",
    "the open work",
    "the open task",
    "the open item",
    "that epic",
    "that thread",
    "that decision",
    "the last decision",
];

const STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "that", "this", "from", "have", "has", "are", "was", "were",
    "can", "you", "our", "your", "what", "where", "how", "why", "when", "will", "would", "should",
    "could", "let", "lets", "please", "about", "into", "its", "not", "but", "all", "any", "now",
    "then", "there", "here", "who", "which", "just", "also", "get", "got", "new", "one", "two",
    "out", "too", "their", "them", "they", "been", "did", "does", "doing", "done", "want", "need",
    "continue", "resume", "open", "work", "thread", "epic", "status", "carry", "pick", "back",
    "last", "thing", "still", "left", "off", "decision",
];

#[derive(Debug, Clone)]
pub(crate) struct ThreadFacts {
    pub(crate) slug: String,
    pub(crate) title: String,
    pub(crate) goal: String,
    pub(crate) status: String,
    pub(crate) branch: Option<String>,
    pub(crate) worktree: Option<String>,
    pub(crate) last_session_uuid: Option<Uuid>,
    pub(crate) last_checkpoint_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub(crate) struct SessionFacts<'a> {
    pub(crate) session_uuid: Uuid,
    pub(crate) predecessor_session_uuid: Option<Uuid>,
    pub(crate) checkout_branch: Option<String>,
    pub(crate) checkout_worktree: Option<String>,
    pub(crate) recent_turn_text: Vec<&'a str>,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolveInput<'a> {
    pub(crate) prompt: &'a str,
    pub(crate) session: SessionFacts<'a>,
    pub(crate) threads: &'a [ThreadFacts],
    pub(crate) now: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Signal {
    ExplicitMention,
    RecentMention,
    BranchMatch,
    WorktreeMatch,
    LastSession,
    RecentCheckpoint,
    PromptWords,
    Deictic,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Pick {
    pub(crate) slug: String,
    pub(crate) score: f64,
    pub(crate) signals: Vec<Signal>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Resolution {
    One(Pick),
    Several(Vec<Pick>),
    None,
}

struct Text {
    lower: String,
    tickets: Vec<String>,
}

impl Text {
    fn new(raw: &str) -> Self {
        Self {
            lower: raw.to_lowercase(),
            tickets: tickets(raw),
        }
    }

    fn mentions(&self, slug: &str, haystack: &str) -> bool {
        contains_bounded(&self.lower, slug)
            || self
                .tickets
                .iter()
                .any(|ticket| contains_bounded(haystack, ticket))
    }
}

struct Context<'a> {
    prompt: Text,
    turns: Text,
    prompt_words: BTreeSet<String>,
    session: &'a SessionFacts<'a>,
    now: DateTime<Utc>,
    deictic: bool,
}

struct Candidate {
    pick: Pick,
    explicit: bool,
    shared_words: usize,
}

pub(crate) fn resolve(input: &ResolveInput<'_>) -> Resolution {
    let ctx = build_context(input);
    let mut candidates: Vec<Candidate> = input
        .threads
        .iter()
        .map(|thread| score_thread(thread, &ctx))
        .filter(|candidate| ctx.deictic || passes_prompt_gate(candidate))
        .filter(|candidate| candidate.pick.score >= MIN_SCORE)
        .collect();
    candidates.sort_by(|a, b| {
        b.pick
            .score
            .total_cmp(&a.pick.score)
            .then_with(|| a.pick.slug.cmp(&b.pick.slug))
    });
    decide(candidates.into_iter().map(|c| c.pick).collect())
}

fn build_context<'a>(input: &'a ResolveInput<'a>) -> Context<'a> {
    let turns = &input.session.recent_turn_text;
    let skip = turns.len().saturating_sub(RECENT_TURN_LIMIT);
    Context {
        prompt: Text::new(input.prompt),
        turns: Text::new(&turns[skip..].join("\n")),
        prompt_words: tokenize(input.prompt),
        session: &input.session,
        now: input.now,
        deictic: is_deictic(input.prompt),
    }
}

fn passes_prompt_gate(candidate: &Candidate) -> bool {
    candidate.explicit || candidate.shared_words >= WORD_OVERLAP_FLOOR
}

fn decide(ranked: Vec<Pick>) -> Resolution {
    let Some(best) = ranked.first().map(|pick| pick.score) else {
        return Resolution::None;
    };
    let close = ranked
        .iter()
        .take(MAX_SEVERAL + 1)
        .take_while(|pick| best - pick.score < LEAD_MARGIN)
        .count();
    let mut picks = ranked;
    picks.truncate(close.min(MAX_SEVERAL));
    match picks.len() {
        0 => Resolution::None,
        1 => picks.pop().map_or(Resolution::None, Resolution::One),
        _ => Resolution::Several(picks),
    }
}

fn score_thread(thread: &ThreadFacts, ctx: &Context<'_>) -> Candidate {
    let slug = thread.slug.to_lowercase();
    let haystack = format!(
        "{slug} {} {} {}",
        thread.title.to_lowercase(),
        thread.goal.to_lowercase(),
        thread.branch.as_deref().unwrap_or("").to_lowercase()
    );
    let shared_words = shared_word_count(thread, ctx);
    let explicit = ctx.prompt.mentions(&slug, &haystack);
    let contributions = [
        (
            Signal::ExplicitMention,
            explicit.then_some(EXPLICIT_MENTION_WEIGHT),
        ),
        (
            Signal::RecentMention,
            ctx.turns
                .mentions(&slug, &haystack)
                .then_some(RECENT_MENTION_WEIGHT),
        ),
        (
            Signal::BranchMatch,
            branch_matches(thread, ctx.session).then_some(BRANCH_WEIGHT),
        ),
        (
            Signal::WorktreeMatch,
            worktree_matches(thread, ctx.session).then_some(WORKTREE_WEIGHT),
        ),
        (
            Signal::LastSession,
            last_session_matches(thread, ctx.session).then_some(LAST_SESSION_WEIGHT),
        ),
        (
            Signal::RecentCheckpoint,
            Some(recency(thread, ctx.now)).filter(|weight| *weight > 0.0),
        ),
        (
            Signal::PromptWords,
            (shared_words > 0).then_some(WORD_WEIGHT * shared_words.min(WORD_COUNT_CAP) as f64),
        ),
    ];
    let mut score = 0.0;
    let mut signals = Vec::new();
    for (signal, weight) in contributions {
        if let Some(weight) = weight {
            score += weight;
            signals.push(signal);
        }
    }
    if ctx.deictic {
        signals.push(Signal::Deictic);
    }
    Candidate {
        pick: Pick {
            slug: thread.slug.clone(),
            score,
            signals,
        },
        explicit,
        shared_words,
    }
}

fn shared_word_count(thread: &ThreadFacts, ctx: &Context<'_>) -> usize {
    let thread_words = tokenize(&format!("{} {} {}", thread.slug, thread.title, thread.goal));
    thread_words.intersection(&ctx.prompt_words).count()
}

fn same_ref(left: Option<&str>, right: Option<&str>) -> bool {
    matches!((left, right), (Some(a), Some(b)) if !a.is_empty() && a == b)
}

fn branch_matches(thread: &ThreadFacts, session: &SessionFacts<'_>) -> bool {
    same_ref(thread.branch.as_deref(), session.checkout_branch.as_deref())
}

fn worktree_matches(thread: &ThreadFacts, session: &SessionFacts<'_>) -> bool {
    let trim = |path: &str| path.trim_end_matches('/').to_owned();
    let thread_path = thread.worktree.as_deref().map(trim);
    let session_path = session.checkout_worktree.as_deref().map(trim);
    same_ref(thread_path.as_deref(), session_path.as_deref())
}

fn last_session_matches(thread: &ThreadFacts, session: &SessionFacts<'_>) -> bool {
    let Some(last) = thread.last_session_uuid else {
        return false;
    };
    last == session.session_uuid || session.predecessor_session_uuid == Some(last)
}

fn recency(thread: &ThreadFacts, now: DateTime<Utc>) -> f64 {
    let seconds = now
        .signed_duration_since(thread.last_checkpoint_at)
        .num_seconds()
        .max(0);
    let days = seconds as f64 / SECONDS_PER_DAY;
    let weight = RECENCY_WEIGHT * 0.5_f64.powf(days / RECENCY_HALF_LIFE_DAYS);
    if weight >= RECENCY_FLOOR { weight } else { 0.0 }
}

fn is_deictic(prompt: &str) -> bool {
    let normalised = prompt.to_lowercase().replace('\u{2019}', "'");
    DEICTIC_PHRASES
        .iter()
        .any(|phrase| contains_bounded(&normalised, phrase))
}

fn tokenize(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|word| {
            word.chars().count() >= MIN_WORD_CHARS && !STOPWORDS.contains(&word.as_str())
        })
        .collect()
}

fn tickets(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .filter(|token| is_ticket(token))
        .map(str::to_lowercase)
        .collect()
}

fn is_ticket(token: &str) -> bool {
    let Some((prefix, number)) = token.rsplit_once('-') else {
        return false;
    };
    prefix.len() >= MIN_TICKET_PREFIX_CHARS
        && prefix.chars().all(|c| c.is_ascii_alphabetic())
        && !number.is_empty()
        && number.chars().all(|c| c.is_ascii_digit())
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '-' || c == '_'
}

fn contains_bounded(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    haystack.match_indices(needle).any(|(start, matched)| {
        let before = haystack[..start].chars().next_back();
        let after = haystack[start + matched.len()..].chars().next();
        !before.is_some_and(is_word_char) && !after.is_some_and(is_word_char)
    })
}

#[cfg(test)]
#[path = "resolve_tests.rs"]
mod tests;
