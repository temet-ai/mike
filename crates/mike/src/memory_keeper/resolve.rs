use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::resolve_text::{Text, contains_bounded, is_deictic, tokenize};

const MIN_SCORE: f64 = 0.3;
const LEAD_MARGIN: f64 = 0.15;
const MAX_SEVERAL: usize = 3;
const RECENT_TURN_LIMIT: usize = 3;

const EXPLICIT_MENTION_WEIGHT: f64 = 1.0;
const LINKED_SESSION_WEIGHT: f64 = 0.6;
const BRANCH_WEIGHT: f64 = 0.6;
const WORKTREE_WEIGHT: f64 = 0.5;
const RECENT_MENTION_WEIGHT: f64 = 0.45;
const CHECKOUT_WEIGHT: f64 = 0.4;

const RECENCY_WEIGHT: f64 = 0.5;
const RECENCY_HALF_LIFE_DAYS: f64 = 3.0;
const RECENCY_FLOOR: f64 = 0.15;
const SECONDS_PER_DAY: f64 = 86_400.0;

const WORD_WEIGHT: f64 = 0.2;
const WORD_COUNT_CAP: usize = 3;
const WORD_OVERLAP_FLOOR: usize = 2;

const OPEN_STATUSES: &[&str] = &["active", "implementing", "review", "in-progress"];
const DEFAULT_BRANCHES: &[&str] = &["main", "master"];

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
    pub(crate) linked_sessions: BTreeSet<Uuid>,
    pub(crate) checkout_dirs: BTreeSet<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct SessionFacts<'a> {
    pub(crate) session_uuid: Uuid,
    pub(crate) cwd: Option<&'a str>,
    pub(crate) branch: Option<&'a str>,
    pub(crate) recent_turn_text: Vec<&'a str>,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolveInput<'a> {
    pub(crate) prompt: Option<&'a str>,
    pub(crate) session: SessionFacts<'a>,
    pub(crate) threads: &'a [ThreadFacts],
    pub(crate) now: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Signal {
    ExplicitMention,
    RecentMention,
    LinkedSession,
    CheckoutMatch,
    BranchMatch,
    WorktreeMatch,
    RecentCheckpoint,
    PromptWords,
    Deictic,
}

impl Signal {
    fn anchors(self) -> bool {
        !matches!(
            self,
            Self::RecentCheckpoint | Self::PromptWords | Self::Deictic
        )
    }

    fn is_session_signal(self) -> bool {
        matches!(
            self,
            Self::LinkedSession | Self::CheckoutMatch | Self::BranchMatch | Self::WorktreeMatch
        )
    }
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ask {
    Named,
    Continuation,
    Silent,
}

struct Context<'a> {
    prompt: Text,
    turns: Text,
    prompt_words: BTreeSet<String>,
    session: &'a SessionFacts<'a>,
    now: DateTime<Utc>,
    ask: Ask,
}

struct Candidate {
    pick: Pick,
    shared_words: usize,
}

impl Candidate {
    fn has(&self, wanted: impl Fn(Signal) -> bool) -> bool {
        self.pick.signals.iter().any(|signal| wanted(*signal))
    }

    fn qualifies(&self, ask: Ask) -> bool {
        match ask {
            Ask::Named => {
                self.has(|s| s == Signal::ExplicitMention)
                    || self.shared_words >= WORD_OVERLAP_FLOOR
            }
            Ask::Continuation => true,
            Ask::Silent => self.has(Signal::is_session_signal),
        }
    }
}

pub(crate) fn resolve(input: &ResolveInput<'_>) -> Resolution {
    let ctx = build_context(input);
    let mut candidates: Vec<Candidate> = input
        .threads
        .iter()
        .filter(|thread| OPEN_STATUSES.contains(&thread.status.as_str()))
        .map(|thread| score_thread(thread, &ctx))
        .filter(|candidate| candidate.pick.score >= MIN_SCORE && candidate.qualifies(ctx.ask))
        .collect();
    candidates.sort_by(|a, b| {
        b.pick
            .score
            .total_cmp(&a.pick.score)
            .then_with(|| a.pick.slug.cmp(&b.pick.slug))
    });
    if ctx.ask == Ask::Continuation {
        keep_anchored_unless_alone(&mut candidates);
    }
    decide(candidates.into_iter().map(|c| c.pick).collect())
}

fn keep_anchored_unless_alone(candidates: &mut Vec<Candidate>) {
    if candidates.iter().any(|c| c.has(Signal::anchors)) {
        candidates.retain(|c| c.has(Signal::anchors));
    } else if candidates.len() > 1 {
        candidates.clear();
    }
}

fn build_context<'a>(input: &'a ResolveInput<'a>) -> Context<'a> {
    let turns = &input.session.recent_turn_text;
    let skip = turns.len().saturating_sub(RECENT_TURN_LIMIT);
    let prompt = input.prompt.unwrap_or_default();
    let ask = match input.prompt {
        None => Ask::Silent,
        Some(text) if is_deictic(text) => Ask::Continuation,
        Some(_) => Ask::Named,
    };
    Context {
        prompt: Text::new(prompt),
        turns: Text::new(&turns[skip..].join("\n")),
        prompt_words: tokenize(prompt),
        session: &input.session,
        now: input.now,
        ask,
    }
}

fn decide(ranked: Vec<Pick>) -> Resolution {
    let Some(best) = ranked.first().map(|pick| pick.score) else {
        return Resolution::None;
    };
    let close = ranked
        .iter()
        .take_while(|pick| best - pick.score < LEAD_MARGIN)
        .count();
    if close > MAX_SEVERAL {
        return Resolution::None;
    }
    let mut picks = ranked;
    picks.truncate(close);
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
    let words_weight = WORD_WEIGHT * shared_words.min(WORD_COUNT_CAP) as f64;
    let contributions = [
        (
            Signal::ExplicitMention,
            ctx.prompt.mentions(&slug, &haystack),
            EXPLICIT_MENTION_WEIGHT,
        ),
        (
            Signal::RecentMention,
            ctx.turns.mentions(&slug, &haystack),
            RECENT_MENTION_WEIGHT,
        ),
        (
            Signal::LinkedSession,
            linked(thread, ctx.session),
            LINKED_SESSION_WEIGHT,
        ),
        (
            Signal::CheckoutMatch,
            checkout_matches(thread, ctx.session),
            CHECKOUT_WEIGHT,
        ),
        (
            Signal::BranchMatch,
            branch_matches(thread, ctx.session),
            BRANCH_WEIGHT,
        ),
        (
            Signal::WorktreeMatch,
            worktree_matches(thread, ctx.session),
            WORKTREE_WEIGHT,
        ),
        (
            Signal::RecentCheckpoint,
            recency(thread, ctx.now) > 0.0,
            recency(thread, ctx.now),
        ),
        (Signal::PromptWords, shared_words > 0, words_weight),
    ];
    let mut score = 0.0;
    let mut signals = Vec::new();
    for (signal, present, weight) in contributions {
        if present {
            score += weight;
            signals.push(signal);
        }
    }
    if ctx.ask == Ask::Continuation {
        signals.push(Signal::Deictic);
    }
    Candidate {
        pick: Pick {
            slug: thread.slug.clone(),
            score,
            signals,
        },
        shared_words,
    }
}

fn shared_word_count(thread: &ThreadFacts, ctx: &Context<'_>) -> usize {
    let thread_words = tokenize(&format!("{} {} {}", thread.slug, thread.title, thread.goal));
    thread_words.intersection(&ctx.prompt_words).count()
}

fn linked(thread: &ThreadFacts, session: &SessionFacts<'_>) -> bool {
    thread.linked_sessions.contains(&session.session_uuid)
        || thread.last_session_uuid == Some(session.session_uuid)
}

fn trimmed(path: &str) -> &str {
    path.trim_end_matches('/')
}

fn checkout_matches(thread: &ThreadFacts, session: &SessionFacts<'_>) -> bool {
    let Some(cwd) = session.cwd.map(trimmed).filter(|cwd| !cwd.is_empty()) else {
        return false;
    };
    thread.checkout_dirs.iter().any(|dir| trimmed(dir) == cwd)
}

fn branch_matches(thread: &ThreadFacts, session: &SessionFacts<'_>) -> bool {
    match (thread.branch.as_deref(), session.branch) {
        (Some(ours), Some(theirs)) => {
            !ours.is_empty() && ours == theirs && !DEFAULT_BRANCHES.contains(&ours)
        }
        _ => false,
    }
}

fn worktree_matches(thread: &ThreadFacts, session: &SessionFacts<'_>) -> bool {
    match (thread.worktree.as_deref(), session.cwd) {
        (Some(tree), Some(cwd)) => {
            let tree = trimmed(tree);
            !tree.is_empty() && contains_bounded(trimmed(cwd), tree)
        }
        _ => false,
    }
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

#[cfg(test)]
#[path = "resolve_tests.rs"]
mod tests;
