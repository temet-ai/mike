use chrono::{DateTime, Utc};
use serde::Serialize;

use super::SectionReport;
use super::cut::{body, char_count, cut, keep_last};
use super::sentences::sentence_containing;
use crate::dossier::view::full::{genuine_stream, resolutions, source_request_id, span_of};
use crate::dossier::view::types::{CorrectionSource, DossierError, DossierInputs, Exclusion};
use crate::store::persistence::dossier_view::DossierTurnRow;

pub const ASK_CAP: usize = 1_500;
pub const ASK_KEEP: usize = 12;
pub const DIR_KEEP: usize = 12;
pub const DIR_MIN: usize = 8;
pub const DIR_MAX: usize = 400;
pub const INSTR_CAP: usize = 600;
pub const CORR_KEEP: usize = 6;
pub const CORR_ASK_CAP: usize = 300;
pub const CORR_TEXT_CAP: usize = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JudgeSectionId {
    EarlierOperatorMessages,
    Directives,
    PriorCorrections,
    SimilarCorrections,
    SimilarExchanges,
}

impl JudgeSectionId {
    pub const ALL: [JudgeSectionId; 5] = [
        JudgeSectionId::EarlierOperatorMessages,
        JudgeSectionId::Directives,
        JudgeSectionId::PriorCorrections,
        JudgeSectionId::SimilarCorrections,
        JudgeSectionId::SimilarExchanges,
    ];

    pub fn code(self) -> &'static str {
        match self {
            JudgeSectionId::EarlierOperatorMessages => "asks",
            JudgeSectionId::Directives => "directives",
            JudgeSectionId::PriorCorrections => "corrections",
            JudgeSectionId::SimilarCorrections => "similar",
            JudgeSectionId::SimilarExchanges => "neighbours",
        }
    }

    pub fn heading(self) -> &'static str {
        match self {
            JudgeSectionId::EarlierOperatorMessages => {
                "EARLIER OPERATOR MESSAGES IN THIS SESSION (oldest first, word for word, all sent before this turn ended):"
            }
            JudgeSectionId::Directives => {
                "STANDING DIRECTIVES THE OPERATOR GAVE EARLIER IN THIS SESSION (quoted):"
            }
            JudgeSectionId::PriorCorrections => {
                "EARLIER CORRECTIONS IN THIS SESSION (the operator's own words, each sent before this turn ended):"
            }
            JudgeSectionId::SimilarCorrections => {
                "EARLIER CORRECTIONS THAT RESEMBLE THIS TURN, MOST SIMILAR FIRST (the operator's own words, from this session or others, each sent before this turn ended):"
            }
            JudgeSectionId::SimilarExchanges => {
                "HOW THE OPERATOR REPLIED TO SIMILAR REQUESTS IN OTHER SESSIONS (the operator's own words, each sent before this turn ended; no verdict of any kind):"
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JudgeItem {
    pub text: String,
    pub at: DateTime<Utc>,
    pub source_ref: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JudgeSection {
    pub id: JudgeSectionId,
    pub heading: &'static str,
    pub body: String,
    pub items: Vec<JudgeItem>,
    pub depth: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JudgeView {
    pub subject_turn_id: i64,
    pub sections: Vec<JudgeSection>,
}

pub struct JudgeRender {
    pub view: JudgeView,
    pub reports: Vec<SectionReport>,
    pub text: String,
    pub exclusions: Vec<Exclusion>,
}

fn subject(inputs: &DossierInputs, turn_id: i64) -> Result<&DossierTurnRow, DossierError> {
    let turn = inputs
        .turns
        .iter()
        .find(|t| t.turn_id == turn_id)
        .ok_or(DossierError::UnknownTurn(turn_id))?;
    if turn.closed_at.is_none() {
        return Err(DossierError::TurnNotClosedAt {
            turn_id,
            at: inputs.as_of.at,
        });
    }
    Ok(turn)
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn section(
    id: JudgeSectionId,
    lines: &[String],
    items: Vec<JudgeItem>,
    depth: usize,
) -> JudgeSection {
    JudgeSection {
        id,
        heading: id.heading(),
        body: body(lines),
        items,
        depth: to_u32(depth),
    }
}

fn asks_section(
    inputs: &DossierInputs,
    turn: &DossierTurnRow,
    exclusions: &mut Vec<Exclusion>,
) -> JudgeSection {
    let next = resolutions(inputs)
        .get(&turn.turn_id)
        .and_then(|r| r.next_genuine.as_ref().map(|n| n.request_id));
    let mut dropped = 0;
    let earlier: Vec<_> = genuine_stream(inputs)
        .into_iter()
        .filter(|row| row.request_id < turn.start_request_id)
        .filter(|row| {
            let is_next = Some(row.request_id) == next;
            dropped += u32::from(is_next);
            !is_next
        })
        .collect();
    if dropped > 0 {
        exclusions.push(Exclusion {
            section: "asks",
            reason: "the subject's own next genuine message".to_owned(),
            count: dropped,
        });
    }
    let items: Vec<JudgeItem> = earlier
        .iter()
        .map(|row| JudgeItem {
            text: row.text.clone(),
            at: row.at,
            source_ref: format!("requests.requests:{}", row.request_id),
        })
        .collect();
    let lines = keep_last(
        earlier.iter().map(|row| cut(&row.text, ASK_CAP)).collect(),
        ASK_KEEP,
        "operator messages in this session are",
    );
    section(
        JudgeSectionId::EarlierOperatorMessages,
        &lines,
        items,
        earlier.len(),
    )
}

fn directive_sentences(
    inputs: &DossierInputs,
    turn: &DossierTurnRow,
) -> Vec<(i64, usize, String, JudgeItem)> {
    let mut found: Vec<(i64, usize, String, JudgeItem)> = inputs
        .entries
        .iter()
        .filter(|e| e.kind == "directive" && e.status == "open")
        .filter(|e| matches!(e.source.as_str(), "capture" | "operator"))
        .filter_map(|e| {
            let request_id = source_request_id(&e.source_ref)?;
            if request_id >= turn.start_request_id {
                return None;
            }
            let prompt = inputs.stream.iter().find(|r| r.request_id == request_id)?;
            let (start, _) = span_of(e)?;
            let sentence = sentence_containing(&prompt.text, start)?;
            let item = JudgeItem {
                text: sentence.clone(),
                at: e.effective_at,
                source_ref: e.source_ref.clone(),
            };
            Some((request_id, start, sentence, item))
        })
        .collect();
    found.sort_by_key(|a| (a.0, a.1));
    found
}

fn directives_section(inputs: &DossierInputs, turn: &DossierTurnRow) -> JudgeSection {
    let mut sentences: Vec<String> = Vec::new();
    let mut items = Vec::new();
    let mut too_long = 0;
    for (_, _, sentence, item) in directive_sentences(inputs, turn) {
        if char_count(&sentence) < DIR_MIN || sentences.contains(&sentence) {
            continue;
        }
        if char_count(&sentence) > DIR_MAX {
            too_long += 1;
            continue;
        }
        sentences.push(sentence);
        items.push(item);
    }
    let depth = sentences.len() + too_long + usize::from(inputs.goals.is_some());
    let mut lines = keep_last(sentences, DIR_KEEP, "directive sentences are");
    if too_long > 0 {
        lines.insert(
            0,
            format!(
                "({too_long} directive-matching passages longer than 400 characters are not shown)"
            ),
        );
    }
    if let Some(goals) = &inputs.goals {
        let instructions = goals.instructions.as_deref().unwrap_or("");
        lines.insert(
            0,
            format!("Session instructions: {}", cut(instructions, INSTR_CAP)),
        );
    }
    section(JudgeSectionId::Directives, &lines, items, depth)
}

fn corrections_section(inputs: &DossierInputs, turn: &DossierTurnRow) -> JudgeSection {
    let mut chosen: Vec<_> = inputs
        .corrections
        .iter()
        .filter(|c| match &c.source {
            CorrectionSource::Label { tier, split, .. } => {
                matches!(tier.as_str(), "gold" | "relayed") && split == "dev"
            }
            CorrectionSource::LiveVerdict { .. } => false,
        })
        .filter(|c| {
            c.corrected_turn_id != turn.turn_id
                && c.corrected_created_at < turn.created_at
                && c.correcting_at <= inputs.as_of.at
        })
        .collect();
    chosen.sort_by_key(|c| (c.correcting_at, c.correcting_request_id));
    let texts: Vec<String> = chosen
        .iter()
        .map(|c| {
            format!(
                "Earlier ask: {}\nThe operator's correction: {}",
                cut(&c.corrected_prompt, CORR_ASK_CAP),
                cut(&c.correcting_text, CORR_TEXT_CAP)
            )
        })
        .collect();
    let items = chosen
        .iter()
        .map(|c| JudgeItem {
            text: c.correcting_text.clone(),
            at: c.correcting_at,
            source_ref: format!("requests.requests:{}", c.correcting_request_id),
        })
        .collect();
    let depth = texts.len();
    let lines = keep_last(texts, CORR_KEEP, "corrections in this session are");
    section(JudgeSectionId::PriorCorrections, &lines, items, depth)
}

fn report(section: &JudgeSection) -> SectionReport {
    SectionReport {
        id: section.id.code().to_owned(),
        items_total: section.depth,
        items_full: to_u32(section.items.len()),
        items_compressed: 0,
        items_in_spans: 0,
        chars: to_u32(char_count(&section.body)),
    }
}

pub fn render_judge(inputs: &DossierInputs, turn_id: i64) -> Result<JudgeRender, DossierError> {
    let turn = subject(inputs, turn_id)?;
    let mut exclusions = Vec::new();
    let sections = vec![
        asks_section(inputs, turn, &mut exclusions),
        directives_section(inputs, turn),
        corrections_section(inputs, turn),
        section(JudgeSectionId::SimilarCorrections, &[], Vec::new(), 0),
        section(JudgeSectionId::SimilarExchanges, &[], Vec::new(), 0),
    ];
    let text: String = sections
        .iter()
        .map(|s| format!("\n\n{}\n{}", s.heading, s.body))
        .collect();
    let reports = sections.iter().map(report).collect();
    Ok(JudgeRender {
        view: JudgeView {
            subject_turn_id: turn_id,
            sections,
        },
        reports,
        text,
        exclusions,
    })
}
