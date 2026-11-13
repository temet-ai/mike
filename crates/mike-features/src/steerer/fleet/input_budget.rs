use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};

use tracing::warn;

use super::context::EvalInputs;
use super::verifier_judge::VerifierJudgeContext;
use crate::settings::SharedSettingsOverlay;
use crate::steerer::verifier::{build_accumulated_ask, build_session_action_list};
use crate::store::persistence::memory_ask_ledger::AskLedgerEntryRow;
use crate::store::persistence::session_action_list::{SessionActionCounts, SessionActionRow};

pub mod action_list;

use action_list::project_action_list;

#[cfg(test)]
#[path = "input_budget_tests.rs"]
pub mod tests;

pub const CAPPED_ASK_HEADING: &str = "Accumulated ask (the operator's first ask and the newest asks of this work-thread, oldest first; asks in between are counted):";

pub const CAPPED_ACTION_LIST_HEADING: &str = "Session action list (issue #158 — this session's tool calls, oldest first; older calls are counted in the first line and runs of successful reads, searches and fetches are folded; use this rather than the turn-scoped trajectory digest above when judging whether the SESSION as a whole paid its debts):";

static UNKNOWN_CODE_WARNED: AtomicBool = AtomicBool::new(false);

const LINE_MARKER_RESERVE: usize = 40;
const ASK_BETWEEN_RESERVE: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerifierInputBudget {
    #[default]
    Uncapped,
    V1,
    V1Wide,
}

pub struct SectionCaps {
    operator_ask: usize,
    last_user_message: usize,
    tool_calls_summary: usize,
    eval_text: usize,
    anchor_field: usize,
    diagnostics: usize,
    accumulated_ask: usize,
    ask_entry: usize,
    session_actions: usize,
    action_detail: usize,
    action_rows_loaded: i64,
    prior_fires: usize,
    criteria: usize,
}

const V1_CAPS: SectionCaps = SectionCaps {
    operator_ask: 1_500,
    last_user_message: 1_500,
    tool_calls_summary: 1_000,
    eval_text: 4_000,
    anchor_field: 600,
    diagnostics: 800,
    accumulated_ask: 5_000,
    ask_entry: 1_200,
    session_actions: 4_000,
    action_detail: 240,
    action_rows_loaded: 200,
    prior_fires: 800,
    criteria: 4_500,
};

const V1_WIDE_CAPS: SectionCaps = SectionCaps {
    operator_ask: 3_000,
    last_user_message: 3_000,
    tool_calls_summary: 2_000,
    eval_text: 8_000,
    anchor_field: 1_200,
    diagnostics: 1_600,
    accumulated_ask: 10_000,
    ask_entry: 2_400,
    session_actions: 10_000,
    action_detail: 400,
    action_rows_loaded: 500,
    prior_fires: 1_600,
    criteria: 6_000,
};

impl VerifierInputBudget {
    pub fn from_setting(value: &str) -> Self {
        match value {
            "uncapped" => Self::Uncapped,
            "verifier-input-v1" => Self::V1,
            "verifier-input-v1-wide" => Self::V1Wide,
            other => {
                if !UNKNOWN_CODE_WARNED.swap(true, Ordering::Relaxed) {
                    warn!(
                        value = other,
                        "steerer.verifier.input_budget holds an unknown code; using verifier-input-v1"
                    );
                }
                Self::V1
            }
        }
    }

    pub fn for_steerer_kind(settings: &SharedSettingsOverlay, kind: &str) -> Self {
        if kind == "verifier" {
            Self::from_setting(&settings.load().steerer_verifier_input_budget)
        } else {
            Self::Uncapped
        }
    }

    pub fn is_capped(self) -> bool {
        self.caps().is_some()
    }

    pub fn action_rows_loaded(self) -> Option<i64> {
        self.caps().map(|c| c.action_rows_loaded)
    }

    fn caps(self) -> Option<&'static SectionCaps> {
        match self {
            Self::Uncapped => None,
            Self::V1 => Some(&V1_CAPS),
            Self::V1Wide => Some(&V1_WIDE_CAPS),
        }
    }
}

fn char_len(text: &str) -> usize {
    text.chars().count()
}

fn fit_keep(total: usize, max: usize, marker_len: impl Fn(usize) -> usize) -> usize {
    let mut keep = max;
    loop {
        let next = max.saturating_sub(marker_len(total - keep));
        if next >= keep {
            return keep;
        }
        keep = next;
    }
}

fn more_marker(omitted: usize) -> String {
    format!(" [... {omitted} more characters]")
}

fn earlier_marker(omitted: usize) -> String {
    format!("[... {omitted} earlier characters not shown] ")
}

fn cut_head(text: &str, max: usize) -> Cow<'_, str> {
    let total = char_len(text);
    if total <= max {
        return Cow::Borrowed(text);
    }
    let keep = fit_keep(total, max, |omitted| char_len(&more_marker(omitted)));
    let head: String = text.chars().take(keep).collect();
    Cow::Owned(format!("{head}{}", more_marker(total - keep)))
}

fn cut_tail(text: &str, max: usize) -> Cow<'_, str> {
    let total = char_len(text);
    if total <= max {
        return Cow::Borrowed(text);
    }
    let keep = fit_keep(total, max, |omitted| char_len(&earlier_marker(omitted)));
    let tail: String = text.chars().skip(total - keep).collect();
    Cow::Owned(format!("{}{tail}", earlier_marker(total - keep)))
}

fn every_line_is_an_entry(_line: &str) -> bool {
    true
}

fn starts_criterion(line: &str) -> bool {
    line.starts_with("- ")
        || line.starts_with("* ")
        || line.chars().next().is_some_and(|c| c.is_ascii_digit())
}

fn cut_entries_head(text: &str, max: usize, starts_entry: fn(&str) -> bool) -> Cow<'_, str> {
    if char_len(text) <= max {
        return Cow::Borrowed(text);
    }
    let lines: Vec<&str> = text.lines().collect();
    let mut entry_starts: Vec<usize> = vec![0];
    for (i, line) in lines.iter().enumerate().skip(1) {
        if starts_entry(line) {
            entry_starts.push(i);
        }
    }
    let mut kept_lines = 0;
    let mut used = 0;
    for (k, &start) in entry_starts.iter().enumerate() {
        let end = entry_starts.get(k + 1).copied().unwrap_or(lines.len());
        let entry_chars: usize = lines[start..end].iter().map(|l| char_len(l) + 1).sum();
        if used + entry_chars + LINE_MARKER_RESERVE > max {
            break;
        }
        used += entry_chars;
        kept_lines = end;
    }
    if kept_lines == 0 {
        return cut_head(text, max);
    }
    let omitted = lines.len() - kept_lines;
    if omitted == 0 {
        return Cow::Borrowed(text);
    }
    Cow::Owned(format!(
        "{}\n({omitted} further lines not shown)",
        lines[..kept_lines].join("\n")
    ))
}

pub struct BudgetedInputs<'a> {
    pub operator_ask: Cow<'a, str>,
    pub last_user_message: Cow<'a, str>,
    pub tool_calls_summary: Cow<'a, str>,
    pub eval_text: Cow<'a, str>,
    pub anchor: Option<(Cow<'a, str>, Cow<'a, str>)>,
    pub diagnostics: Option<String>,
}

fn render_diagnostics(inputs: &EvalInputs) -> Option<String> {
    if inputs.diagnostics.is_empty() {
        return None;
    }
    let lines: Vec<String> = inputs
        .diagnostics
        .iter()
        .map(|d| {
            format!(
                "[{}] {} — matched: {:?}",
                d.category.as_str(),
                d.advice,
                d.matched_text
            )
        })
        .collect();
    Some(lines.join("\n"))
}

impl VerifierInputBudget {
    pub fn apply(self, inputs: &EvalInputs) -> BudgetedInputs<'_> {
        let diagnostics = render_diagnostics(inputs);
        let Some(caps) = self.caps() else {
            return BudgetedInputs {
                operator_ask: Cow::Borrowed(&inputs.operator_ask),
                last_user_message: Cow::Borrowed(&inputs.last_user_message),
                tool_calls_summary: Cow::Borrowed(&inputs.tool_calls_summary),
                eval_text: Cow::Borrowed(&inputs.eval_text),
                anchor: inputs.anchor.as_ref().map(|a| {
                    (
                        Cow::Borrowed(a.state_of_play.as_str()),
                        Cow::Borrowed(a.next.as_str()),
                    )
                }),
                diagnostics,
            };
        };
        BudgetedInputs {
            operator_ask: cut_head(&inputs.operator_ask, caps.operator_ask),
            last_user_message: cut_head(&inputs.last_user_message, caps.last_user_message),
            tool_calls_summary: cut_entries_head(
                &inputs.tool_calls_summary,
                caps.tool_calls_summary,
                every_line_is_an_entry,
            ),
            eval_text: cut_tail(&inputs.eval_text, caps.eval_text),
            anchor: inputs.anchor.as_ref().map(|a| {
                (
                    cut_head(&a.state_of_play, caps.anchor_field),
                    cut_head(&a.next, caps.anchor_field),
                )
            }),
            diagnostics: diagnostics.map(|d| {
                cut_entries_head(&d, caps.diagnostics, every_line_is_an_entry).into_owned()
            }),
        }
    }
}

pub struct RawVerifierContext {
    pub ask_entries: Vec<AskLedgerEntryRow>,
    pub trajectory_digest: String,
    pub action_rows: Vec<SessionActionRow>,
    pub action_counts: Option<SessionActionCounts>,
    pub thread_criteria: Option<String>,
    pub intake_criteria: Option<String>,
}

pub fn project_verifier_context(
    raw: &RawVerifierContext,
    budget: VerifierInputBudget,
) -> VerifierJudgeContext {
    let Some(caps) = budget.caps() else {
        return VerifierJudgeContext {
            accumulated_ask: build_accumulated_ask(&raw.ask_entries),
            trajectory_digest: raw.trajectory_digest.clone(),
            thread_criteria: raw.thread_criteria.clone(),
            intake_criteria: raw.intake_criteria.clone(),
            session_action_list: build_session_action_list(&raw.action_rows),
            budget,
            ..Default::default()
        };
    };
    let cut_criteria =
        |c: &String| cut_entries_head(c, caps.criteria, starts_criterion).into_owned();
    VerifierJudgeContext {
        accumulated_ask: project_accumulated_ask(&raw.ask_entries, caps),
        trajectory_digest: raw.trajectory_digest.clone(),
        thread_criteria: raw.thread_criteria.as_ref().map(cut_criteria),
        intake_criteria: raw.intake_criteria.as_ref().map(cut_criteria),
        session_action_list: project_action_list(&raw.action_rows, raw.action_counts, caps),
        budget,
        ..Default::default()
    }
}

pub fn project_prior_fires(digest: String, budget: VerifierInputBudget) -> String {
    match budget.caps() {
        Some(caps) => {
            cut_entries_head(&digest, caps.prior_fires, every_line_is_an_entry).into_owned()
        }
        None => digest,
    }
}

fn ask_line(entry: &AskLedgerEntryRow, caps: &SectionCaps) -> String {
    format!(
        "[{}] {}",
        entry.kind,
        cut_head(&entry.content, caps.ask_entry)
    )
}

fn project_accumulated_ask(entries: &[AskLedgerEntryRow], caps: &SectionCaps) -> String {
    let lines: Vec<String> = entries.iter().map(|e| ask_line(e, caps)).collect();
    let Some(first) = lines.first() else {
        return String::new();
    };
    let joined_len: usize = lines.iter().map(|l| char_len(l) + 1).sum::<usize>() - 1;
    if joined_len <= caps.accumulated_ask {
        return lines.join("\n");
    }
    let mut room = caps
        .accumulated_ask
        .saturating_sub(char_len(first) + 1 + ASK_BETWEEN_RESERVE);
    let mut newest: Vec<&String> = Vec::new();
    for line in lines.iter().skip(1).rev() {
        let need = char_len(line) + 1;
        if need > room {
            break;
        }
        room -= need;
        newest.push(line);
    }
    newest.reverse();
    let between = lines.len() - 1 - newest.len();
    let mut out = vec![first.clone()];
    if between > 0 {
        out.push(format!(
            "({between} asks between the first and the newest are not shown)"
        ));
    }
    out.extend(newest.into_iter().cloned());
    out.join("\n")
}
