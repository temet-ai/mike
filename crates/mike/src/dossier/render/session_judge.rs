use std::collections::HashMap;

use serde::Serialize;

use super::SectionReport;
use super::cut::{char_count, cut, cut_front, head_chars};
use crate::dossier::genuine::PromptKind;
use crate::dossier::view::full::effective_by_request;
use crate::dossier::view::types::{DossierInputs, SessionSlices};
use crate::store::persistence::dossier_view::DossierTurnRow;

pub const RESPONSE_TAIL_CAP: usize = 3_000;
const COMPRESSED_HEAD: usize = 200;
const SEPARATOR: &str = "\n\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnForm {
    Full,
    Compressed,
    Span,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SessionJudgeView {
    pub ending_turn_id: Option<i64>,
    pub forms: Vec<(i64, TurnForm)>,
}

pub struct SessionJudgeRender {
    pub view: SessionJudgeView,
    pub reports: Vec<SectionReport>,
    pub text: String,
}

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn hhmm(turn: &DossierTurnRow) -> String {
    turn.created_at.format("%H:%M").to_string()
}

fn checks(turn: &DossierTurnRow) -> String {
    format!(
        "checks {}/{}",
        turn.checks_count.unwrap_or(0),
        turn.checks_failed.unwrap_or(0)
    )
}

fn reaction_suffix(reactions: &HashMap<i64, String>, turn: &DossierTurnRow) -> String {
    reactions
        .get(&turn.turn_id)
        .map(|r| format!(" reaction={r}"))
        .unwrap_or_default()
}

fn response_of(turn: &DossierTurnRow, cap: usize) -> String {
    let cut_at = turn
        .text_cut_at
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(usize::MAX);
    let body = head_chars(&turn.final_text, cut_at);
    cut_front(body, cap)
}

fn full_form(n: usize, turn: &DossierTurnRow, budget: usize, cap: usize, suffix: &str) -> String {
    let prompt_cap = (budget / 4).max(1);
    let prompt = if char_count(&turn.prompt) > prompt_cap {
        format!(
            "{} (prompt_chars_total={})",
            cut(&turn.prompt, prompt_cap),
            char_count(&turn.prompt)
        )
    } else {
        turn.prompt.clone()
    };
    format!(
        "#{n} {}\nPROMPT:\n{prompt}\nRESPONSE:\n{}\nFACTS: stop_reason={}, aborted={}, interrupted={}, {}, ended_on={}{suffix}",
        hhmm(turn),
        response_of(turn, cap),
        turn.stop_reason.as_deref().unwrap_or("unknown"),
        turn.aborted.unwrap_or(false),
        turn.interrupted.unwrap_or(false),
        checks(turn),
        turn.ended_on.as_deref().unwrap_or("none"),
    )
}

fn compressed_form(n: usize, turn: &DossierTurnRow, suffix: &str) -> String {
    format!(
        "#{n} {} {} → ended_on={}, {}{suffix}",
        hhmm(turn),
        head_chars(&collapse(&turn.prompt), COMPRESSED_HEAD),
        turn.ended_on.as_deref().unwrap_or("none"),
        checks(turn)
    )
}

fn span_form(first: usize, last: usize, turns: &[&DossierTurnRow]) -> String {
    let aborted = turns.iter().filter(|t| t.aborted.unwrap_or(false)).count();
    let failed = turns
        .iter()
        .filter(|t| t.checks_failed.unwrap_or(0) > 0)
        .count();
    format!(
        "turns {first}-{last}: {} turns, {aborted} aborted, {failed} with failed checks",
        turns.len()
    )
}

fn genuine_turns(inputs: &DossierInputs) -> Vec<&DossierTurnRow> {
    let effective = effective_by_request(inputs);
    inputs
        .turns
        .iter()
        .filter(|t| {
            effective
                .get(&t.start_request_id)
                .is_some_and(|e| e.kind == PromptKind::Genuine)
        })
        .collect()
}

fn header(inputs: &DossierInputs, genuine: &[&DossierTurnRow]) -> String {
    let machine = inputs
        .turns
        .iter()
        .filter(|t| matches!(t.opened_by.as_deref(), Some("machine" | "agent")))
        .count();
    let facts = inputs
        .turns
        .iter()
        .rev()
        .find(|t| t.session_class.is_some());
    format!(
        "SESSION\nproject: {}\nfirst genuine prompt: {}\nlast genuine prompt: {}\ngenuine turns: {}\nmachine-opened turns: {machine}\nsession class: {} ({}; facts built {})",
        inputs
            .session
            .project_path
            .as_deref()
            .unwrap_or("(no project)"),
        genuine
            .first()
            .map_or("-".to_owned(), |t| t.created_at.to_rfc3339()),
        genuine
            .last()
            .map_or("-".to_owned(), |t| t.created_at.to_rfc3339()),
        genuine.len(),
        facts
            .and_then(|t| t.session_class.as_deref())
            .unwrap_or("unknown"),
        facts
            .and_then(|t| t.session_class_rule.as_deref())
            .unwrap_or("-"),
        facts
            .and_then(|t| t.built_at)
            .map_or("-".to_owned(), |b| b.to_rfc3339()),
    )
}

struct Layout<'a> {
    turns: Vec<&'a DossierTurnRow>,
    reactions: HashMap<i64, String>,
    budget: usize,
}

impl Layout<'_> {
    fn line(&self, index: usize, form: TurnForm) -> String {
        let turn = self.turns[index];
        let suffix = reaction_suffix(&self.reactions, turn);
        match form {
            TurnForm::Full => full_form(index + 1, turn, self.budget, RESPONSE_TAIL_CAP, &suffix),
            TurnForm::Compressed => compressed_form(index + 1, turn, &suffix),
            TurnForm::Span => String::new(),
        }
    }

    fn render(&self, forms: &[TurnForm], ending_cap: usize) -> Vec<String> {
        let mut out = Vec::new();
        let mut i = 0;
        let last = self.turns.len().saturating_sub(1);
        while i < forms.len() {
            if forms[i] == TurnForm::Span {
                let start = i;
                while i < forms.len() && forms[i] == TurnForm::Span {
                    i += 1;
                }
                out.push(span_form(start + 1, i, &self.turns[start..i]));
                continue;
            }
            if i == last && forms[i] == TurnForm::Full {
                let suffix = reaction_suffix(&self.reactions, self.turns[i]);
                out.push(full_form(
                    i + 1,
                    self.turns[i],
                    self.budget,
                    ending_cap,
                    &suffix,
                ));
            } else {
                out.push(self.line(i, forms[i]));
            }
            i += 1;
        }
        out
    }
}

fn total(parts: &[String]) -> usize {
    let chars: usize = parts.iter().map(|p| char_count(p)).sum();
    chars + SEPARATOR.len() * parts.len().saturating_sub(1)
}

fn fits(layout: &Layout<'_>, head: &str, forms: &[TurnForm], cap: usize) -> bool {
    let mut parts = vec![head.to_owned()];
    parts.extend(layout.render(forms, cap));
    total(&parts) <= layout.budget
}

fn allocate(layout: &Layout<'_>, head: &str) -> (Vec<TurnForm>, usize) {
    let n = layout.turns.len();
    let mut forms = vec![TurnForm::Compressed; n];
    if n == 0 {
        return (forms, RESPONSE_TAIL_CAP);
    }
    forms[n - 1] = TurnForm::Full;
    if fits(layout, head, &forms, RESPONSE_TAIL_CAP) {
        for i in (0..n - 1).rev() {
            forms[i] = TurnForm::Full;
            if !fits(layout, head, &forms, RESPONSE_TAIL_CAP) {
                forms[i] = TurnForm::Compressed;
                break;
            }
        }
        return (forms, RESPONSE_TAIL_CAP);
    }
    for i in 0..n - 1 {
        forms[i] = TurnForm::Span;
        if fits(layout, head, &forms, RESPONSE_TAIL_CAP) {
            return (forms, RESPONSE_TAIL_CAP);
        }
    }
    let mut cap = RESPONSE_TAIL_CAP;
    while cap > 0 && !fits(layout, head, &forms, cap) {
        cap /= 2;
    }
    (forms, cap)
}

fn count(forms: &[TurnForm], form: TurnForm) -> u32 {
    u32::try_from(forms.iter().filter(|f| **f == form).count()).unwrap_or(u32::MAX)
}

pub fn render_session_judge(
    inputs: &DossierInputs,
    slices: SessionSlices,
    budget: usize,
) -> SessionJudgeRender {
    let genuine = genuine_turns(inputs);
    let head = header(inputs, &genuine);
    let reactions: HashMap<i64, String> = if slices.reactions {
        inputs
            .reactions
            .iter()
            .map(|r| (r.turn_id, r.reaction.clone()))
            .collect()
    } else {
        HashMap::new()
    };
    let layout = Layout {
        turns: genuine,
        reactions,
        budget,
    };
    let (forms, cap) = allocate(&layout, &head);
    let turn_lines = layout.render(&forms, cap);
    let mut parts = vec![head.clone()];
    parts.extend(turn_lines.iter().cloned());
    let mut reports = vec![
        SectionReport::whole("session", 1, char_count(&head)),
        SectionReport {
            id: "turns".to_owned(),
            items_total: u32::try_from(forms.len()).unwrap_or(u32::MAX),
            items_full: count(&forms, TurnForm::Full),
            items_compressed: count(&forms, TurnForm::Compressed),
            items_in_spans: count(&forms, TurnForm::Span),
            chars: u32::try_from(total(&turn_lines)).unwrap_or(u32::MAX),
        },
    ];
    for (id, lines) in slice_sections(inputs, slices) {
        let text = lines.join("\n");
        reports.push(SectionReport::whole(
            id,
            lines.len().saturating_sub(1),
            char_count(&text),
        ));
        parts.push(text);
    }
    SessionJudgeRender {
        view: SessionJudgeView {
            ending_turn_id: layout.turns.last().map(|t| t.turn_id),
            forms: layout.turns.iter().map(|t| t.turn_id).zip(forms).collect(),
        },
        reports,
        text: parts.join(SEPARATOR),
    }
}

fn slice_sections(
    inputs: &DossierInputs,
    slices: SessionSlices,
) -> Vec<(&'static str, Vec<String>)> {
    let mut out = Vec::new();
    if slices.promises {
        let mut lines = vec!["PROMISES (open or broken as of now):".to_owned()];
        lines.extend(
            inputs
                .entries
                .iter()
                .filter(|e| e.kind == "promise" && matches!(e.status.as_str(), "open" | "broken"))
                .map(|e| {
                    format!(
                        "- turn {} {}: {}",
                        e.operator_turn_id.unwrap_or(0),
                        e.status,
                        e.data
                    )
                }),
        );
        out.push(("promises", lines));
    }
    if slices.directives {
        let mut lines = vec!["DIRECTIVES IN FORCE:".to_owned()];
        lines.extend(
            inputs
                .entries
                .iter()
                .filter(|e| e.kind == "directive" && e.status == "open")
                .map(|e| {
                    format!(
                        "- {} {} ({})",
                        e.data.get("class").and_then(|v| v.as_str()).unwrap_or("?"),
                        e.data
                            .get("polarity")
                            .and_then(|v| v.as_str())
                            .unwrap_or("?"),
                        e.source_ref
                    )
                }),
        );
        out.push(("directives", lines));
    }
    if slices.reactions {
        out.push((
            "reactions",
            vec!["REACTIONS: shown on each turn's line.".to_owned()],
        ));
    }
    out
}
