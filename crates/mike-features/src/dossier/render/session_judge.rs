use std::collections::HashMap;

use serde::Serialize;

use super::SectionReport;
use super::cut::{char_count, keep_last};
use super::session_sections::{
    DIRECTIVES_HEADING, LESSONS_HEADING, Numbers, PROMISES_HEADING, RECAPS_HEADING,
    directive_lines, lesson_lines, promise_lines, recap_lines, section,
};
use super::session_turn::{
    Detail, MIN_ENDING_STOP_CAP, SHORT_STOP_CAP, STOP_CAP, TurnStyle, is_operator, render_turn,
};
use crate::dossier::view::types::DossierInputs;
use crate::store::persistence::dossier_view::RecordTurnRow;

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

fn header(inputs: &DossierInputs) -> String {
    let turns = &inputs.record.turns;
    let genuine: Vec<&RecordTurnRow> = turns.iter().filter(|t| is_operator(t)).collect();
    let machine = turns.len() - genuine.len();
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
            .map_or("-".to_owned(), |t| t.opened_at.to_rfc3339()),
        genuine
            .last()
            .map_or("-".to_owned(), |t| t.opened_at.to_rfc3339()),
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

pub fn turns_text(text: &str) -> &str {
    let marker = format!("{SEPARATOR}{RECAPS_HEADING}");
    text.rfind(&marker).map_or(text, |at| &text[..at])
}

fn span_line(first: usize, last: usize, turns: &[RecordTurnRow]) -> String {
    let aborted = turns.iter().filter(|t| t.aborted.unwrap_or(false)).count();
    let failed = turns
        .iter()
        .filter(|t| t.checks_failed.unwrap_or(0) > 0)
        .count();
    let commits: i32 = turns.iter().map(|t| t.commit_count).sum();
    format!(
        "turns {first}-{last}: {} turns, {aborted} aborted, {failed} with failed checks, {commits} commits",
        turns.len()
    )
}

#[derive(Debug, Clone, Copy)]
struct Shape {
    shortened: usize,
    merged: usize,
    recaps: usize,
    lessons: usize,
    ending_cap: usize,
}

struct Plan<'a> {
    turns: &'a [RecordTurnRow],
    head: String,
    reactions: HashMap<i64, String>,
    recaps: Vec<String>,
    lessons: Vec<String>,
    promises: String,
    promise_items: usize,
    directives: String,
    directive_items: usize,
    full: Vec<String>,
    short: Vec<String>,
    full_len: Vec<usize>,
    short_len: Vec<usize>,
    budget: usize,
}

fn total(parts: &[String]) -> usize {
    let chars: usize = parts.iter().map(|p| char_count(p)).sum();
    chars + SEPARATOR.len() * parts.len().saturating_sub(1)
}

impl<'a> Plan<'a> {
    fn new(inputs: &'a DossierInputs, budget: usize) -> Self {
        let turns = inputs.record.turns.as_slice();
        let numbers = Numbers::new(turns.iter().map(|t| t.turn_id));
        let reactions: HashMap<i64, String> = inputs
            .reactions
            .iter()
            .map(|r| (r.turn_id, r.reaction.clone()))
            .collect();
        let promises = promise_lines(&inputs.entries, &numbers);
        let directives = directive_lines(&inputs.entries);
        let mut plan = Self {
            turns,
            head: header(inputs),
            reactions,
            recaps: recap_lines(&inputs.record.recaps),
            lessons: lesson_lines(&inputs.record.lessons, &numbers),
            promise_items: promises.len(),
            promises: section(PROMISES_HEADING, promises),
            directive_items: directives.len(),
            directives: section(DIRECTIVES_HEADING, directives),
            full: Vec::new(),
            short: Vec::new(),
            full_len: Vec::new(),
            short_len: Vec::new(),
            budget,
        };
        plan.full = (0..turns.len())
            .map(|i| plan.turn_text(i, Detail::Full, STOP_CAP))
            .collect();
        plan.short = (0..turns.len())
            .map(|i| plan.turn_text(i, Detail::Short, SHORT_STOP_CAP))
            .collect();
        plan.full_len = plan.full.iter().map(|t| char_count(t)).collect();
        plan.short_len = plan.short.iter().map(|t| char_count(t)).collect();
        plan
    }

    fn turn_text(&self, index: usize, detail: Detail, stop_cap: usize) -> String {
        let turn = &self.turns[index];
        let style = TurnStyle {
            number: index + 1,
            detail,
            stop_cap,
            budget: self.budget,
            reaction: self.reactions.get(&turn.turn_id).map(String::as_str),
        };
        render_turn(turn, &style)
    }

    fn detail_of(&self, index: usize, shape: &Shape) -> Detail {
        if index < shape.shortened {
            Detail::Short
        } else {
            Detail::Full
        }
    }

    fn turn_part(&self, index: usize, shape: &Shape) -> String {
        let detail = self.detail_of(index, shape);
        if index + 1 == self.turns.len() {
            let cap = detail.stop_cap().min(shape.ending_cap);
            return self.turn_text(index, detail, cap);
        }
        match detail {
            Detail::Full => self.full[index].clone(),
            Detail::Short => self.short[index].clone(),
        }
    }

    fn turn_parts(&self, shape: &Shape) -> Vec<String> {
        let mut parts = Vec::new();
        if shape.merged > 0 {
            parts.push(span_line(1, shape.merged, &self.turns[..shape.merged]));
        }
        for index in shape.merged..self.turns.len() {
            parts.push(self.turn_part(index, shape));
        }
        parts
    }

    fn list_section(&self, heading: &str, lines: &[String], keep: usize, noun: &str) -> String {
        section(heading, keep_last(lines.to_vec(), keep, noun))
    }

    fn section_texts(&self, shape: &Shape) -> [String; 4] {
        [
            self.list_section(RECAPS_HEADING, &self.recaps, shape.recaps, "recaps are"),
            self.list_section(LESSONS_HEADING, &self.lessons, shape.lessons, "lessons are"),
            self.promises.clone(),
            self.directives.clone(),
        ]
    }

    fn size(&self, shape: &Shape) -> usize {
        let n = self.turns.len();
        let mut chars = char_count(&self.head);
        let mut parts = 1;
        if shape.merged > 0 {
            chars += char_count(&span_line(1, shape.merged, &self.turns[..shape.merged]));
            parts += 1;
        }
        for index in shape.merged..n {
            chars += match (index + 1 == n, self.detail_of(index, shape)) {
                (true, _) => char_count(&self.turn_part(index, shape)),
                (false, Detail::Full) => self.full_len[index],
                (false, Detail::Short) => self.short_len[index],
            };
            parts += 1;
        }
        let sections = self.section_texts(shape);
        chars += sections.iter().map(|s| char_count(s)).sum::<usize>();
        parts += sections.len();
        chars + SEPARATOR.len() * (parts - 1)
    }

    fn fits(&self, shape: &Shape) -> bool {
        self.size(shape) <= self.budget
    }

    fn allocate(&self) -> Shape {
        let n = self.turns.len();
        let mut shape = Shape {
            shortened: 0,
            merged: 0,
            recaps: self.recaps.len(),
            lessons: self.lessons.len(),
            ending_cap: usize::MAX,
        };
        while !self.fits(&shape) {
            if !step(&mut shape, n) {
                break;
            }
        }
        shape
    }
}

fn step(shape: &mut Shape, turns: usize) -> bool {
    if shape.shortened < turns {
        shape.shortened += 1;
    } else if shape.lessons > 0 {
        shape.lessons -= 1;
    } else if shape.recaps > 0 {
        shape.recaps -= 1;
    } else if shape.merged + 1 < turns {
        shape.merged += 1;
    } else {
        let current = shape.ending_cap.min(SHORT_STOP_CAP);
        if current <= MIN_ENDING_STOP_CAP {
            return false;
        }
        shape.ending_cap = (current / 2).max(MIN_ENDING_STOP_CAP);
    }
    true
}

fn count(forms: &[TurnForm], form: TurnForm) -> u32 {
    u32::try_from(forms.iter().filter(|f| **f == form).count()).unwrap_or(u32::MAX)
}

fn forms_of(turns: usize, shape: &Shape) -> Vec<TurnForm> {
    (0..turns)
        .map(|i| {
            if i < shape.merged {
                TurnForm::Span
            } else if i < shape.shortened {
                TurnForm::Compressed
            } else {
                TurnForm::Full
            }
        })
        .collect()
}

fn list_report(id: &str, total_items: usize, kept: usize, text: &str) -> SectionReport {
    let kept = kept.min(total_items);
    SectionReport {
        id: id.to_owned(),
        items_total: u32::try_from(total_items).unwrap_or(u32::MAX),
        items_full: u32::try_from(kept).unwrap_or(u32::MAX),
        items_compressed: 0,
        items_in_spans: u32::try_from(total_items - kept).unwrap_or(u32::MAX),
        chars: u32::try_from(char_count(text)).unwrap_or(u32::MAX),
    }
}

fn reports(
    plan: &Plan<'_>,
    shape: &Shape,
    forms: &[TurnForm],
    turn_parts: &[String],
    sections: &[String; 4],
) -> Vec<SectionReport> {
    let [recaps, lessons, promises, directives] = sections;
    vec![
        SectionReport::whole("session", 1, char_count(&plan.head)),
        SectionReport {
            id: "turns".to_owned(),
            items_total: u32::try_from(forms.len()).unwrap_or(u32::MAX),
            items_full: count(forms, TurnForm::Full),
            items_compressed: count(forms, TurnForm::Compressed),
            items_in_spans: count(forms, TurnForm::Span),
            chars: u32::try_from(total(turn_parts)).unwrap_or(u32::MAX),
        },
        list_report("recaps", plan.recaps.len(), shape.recaps, recaps),
        list_report("lessons", plan.lessons.len(), shape.lessons, lessons),
        SectionReport::whole("promises", plan.promise_items, char_count(promises)),
        SectionReport::whole("directives", plan.directive_items, char_count(directives)),
    ]
}

pub fn render_session_judge(inputs: &DossierInputs, budget: usize) -> SessionJudgeRender {
    let plan = Plan::new(inputs, budget);
    let shape = plan.allocate();
    let turn_parts = plan.turn_parts(&shape);
    let sections = plan.section_texts(&shape);
    let forms = forms_of(plan.turns.len(), &shape);
    let reports = reports(&plan, &shape, &forms, &turn_parts, &sections);
    let mut parts = vec![plan.head.clone()];
    parts.extend(turn_parts);
    parts.extend(sections);
    SessionJudgeRender {
        view: SessionJudgeView {
            ending_turn_id: plan.turns.last().map(|t| t.turn_id),
            forms: plan.turns.iter().map(|t| t.turn_id).zip(forms).collect(),
        },
        reports,
        text: parts.join(SEPARATOR),
    }
}
