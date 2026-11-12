use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::Serialize;

use super::allocate::{AllocSection, Allocated, SectionBody, allocate};
use super::cut::{cut, head_chars};
use super::judge::{CORR_ASK_CAP, CORR_TEXT_CAP};
use crate::dossier::view::full::{
    EntryItem, full_view, genuine_stream, open_turn_text, resolutions,
};
use crate::dossier::view::types::{
    CorrectionSource, DossierInputs, Polarity, Reaction, SteererSection,
};
use crate::wire::genuine::operator_words;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecentReaction {
    pub operator_turn_id: i64,
    pub reaction: Option<Reaction>,
    pub turn_verdict_id: Option<i64>,
    pub rationale: Option<String>,
    pub correcting_prompt: String,
    pub prompt_request_id: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DirectiveInForce {
    pub entry_id: i64,
    pub class: String,
    pub polarity: Polarity,
    pub source_turn_id: Option<i64>,
    pub source_ref: String,
    pub text_span: String,
    pub effective_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SteererView {
    pub recent_reactions: Vec<RecentReaction>,
    pub directives: Vec<DirectiveInForce>,
    pub steering_facts: Option<crate::dossier::steering_facts::SteeringFacts>,
    pub semantic: Option<crate::semantic::sections::SemanticSections>,
    pub rendered: String,
}

pub struct SteererRender {
    pub view: SteererView,
    pub allocated: Allocated,
}

const RECENT: usize = 3;

pub const RECENT_REACTIONS_HEADING: &str = "RECENT OPERATOR REACTIONS (newest first):";

pub fn recent_reactions(inputs: &DossierInputs) -> Vec<RecentReaction> {
    let resolved = resolutions(inputs);
    let prompts: HashMap<i64, &str> = inputs
        .stream
        .iter()
        .map(|r| (r.request_id, r.text.as_str()))
        .collect();
    let verdicts: HashMap<i64, _> = inputs.reactions.iter().map(|r| (r.turn_id, r)).collect();
    let mut closed: Vec<_> = inputs
        .turns
        .iter()
        .filter(|t| t.closed_at.is_some() && t.opened_by.as_deref() != Some("machine"))
        .collect();
    closed.sort_by_key(|t| (t.closed_at, t.turn_id));
    closed
        .iter()
        .rev()
        .filter_map(|turn| {
            let next = resolved.get(&turn.turn_id)?.next_genuine.as_ref()?;
            let verdict = verdicts.get(&turn.turn_id);
            Some(RecentReaction {
                operator_turn_id: turn.turn_id,
                reaction: verdict.and_then(|v| Reaction::from_code(&v.reaction)),
                turn_verdict_id: verdict.map(|v| v.verdict_id),
                rationale: verdict.and_then(|v| v.rationale.clone()),
                correcting_prompt: prompts
                    .get(&next.request_id)
                    .copied()
                    .unwrap_or("")
                    .to_owned(),
                prompt_request_id: next.request_id,
            })
        })
        .take(RECENT)
        .collect()
}

fn polarity(entry: &EntryItem) -> Polarity {
    match entry.entry.data.get("polarity").and_then(|v| v.as_str()) {
        Some("forbid") => Polarity::Forbid,
        _ => Polarity::Allow,
    }
}

pub fn directives_in_force(inputs: &DossierInputs) -> Vec<DirectiveInForce> {
    full_view(inputs)
        .directives
        .iter()
        .filter(|d| d.status_at == "open")
        .map(|d| DirectiveInForce {
            entry_id: d.entry.id,
            class: d
                .entry
                .data
                .get("class")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned(),
            polarity: polarity(d),
            source_turn_id: d.entry.operator_turn_id,
            source_ref: d.entry.source_ref.clone(),
            text_span: d.text_span.clone().unwrap_or_default(),
            effective_at: d.entry.effective_at,
        })
        .collect()
}

fn fmt_time(at: DateTime<Utc>) -> String {
    at.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn ending_text(inputs: &DossierInputs) -> String {
    if let Some(open) = open_turn_text(inputs) {
        return open.text;
    }
    inputs
        .turns
        .iter()
        .rev()
        .find(|t| t.closed_at.is_some())
        .map(|t| {
            let cut_at = t
                .text_cut_at
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or(usize::MAX);
            head_chars(&t.final_text, cut_at).to_owned()
        })
        .unwrap_or_default()
}

fn reaction_block(r: &RecentReaction, inputs: &DossierInputs) -> String {
    let at = inputs
        .turns
        .iter()
        .find(|t| t.turn_id == r.operator_turn_id)
        .and_then(|t| t.closed_at)
        .map_or_else(String::new, |at| at.format("%H:%M:%S").to_string());
    let mut block = format!(
        "turn {} at {at}Z: reaction {}\nThe operator's reply, word for word: {}",
        r.operator_turn_id,
        r.reaction.map_or("no live verdict", Reaction::code),
        operator_words(&r.correcting_prompt)
    );
    if let Some(rationale) = &r.rationale {
        block.push_str(&format!("\nVerdict rationale: {rationale}"));
    }
    block
}

fn promise_line(entry: &EntryItem) -> String {
    let data = &entry.entry.data;
    let backing = match data.get("backing").and_then(|v| v.as_array()) {
        None => "registry had no coverage".to_owned(),
        Some(items) if items.is_empty() => "none recorded".to_owned(),
        Some(items) => items
            .iter()
            .map(|i| {
                format!(
                    "{} {}",
                    i.get("kind").and_then(|v| v.as_str()).unwrap_or("?"),
                    i.get("task_id").and_then(|v| v.as_str()).unwrap_or("?")
                )
            })
            .collect::<Vec<_>>()
            .join(", "),
    };
    format!(
        "- turn {} at {}: \"{}\"; registry items: {backing}",
        entry.entry.operator_turn_id.unwrap_or(0),
        fmt_time(entry.entry.effective_at),
        entry.text_span.as_deref().unwrap_or("")
    )
}

fn list(id: &'static str, heading: &str, items: Vec<String>, noun: &'static str) -> AllocSection {
    AllocSection {
        id,
        heading: heading.to_owned(),
        body: SectionBody::List { items, noun },
    }
}

fn directive_section(inputs: &DossierInputs, classes: Option<&Vec<String>>) -> AllocSection {
    let items = directives_in_force(inputs)
        .into_iter()
        .filter(|d| classes.is_none_or(|c| c.contains(&d.class)))
        .map(|d| {
            format!(
                "- {} {}, turn {} at {}: \"{}\"",
                d.class,
                if d.polarity == Polarity::Forbid {
                    "forbid"
                } else {
                    "allow"
                },
                d.source_turn_id.unwrap_or(0),
                fmt_time(d.effective_at),
                d.text_span
            )
        })
        .collect();
    list(
        "directives",
        "STANDING DIRECTIVES IN FORCE (the operator's words):",
        items,
        "directives are",
    )
}

fn asks_section(inputs: &DossierInputs, turn_id: Option<i64>) -> AllocSection {
    let before = turn_id
        .and_then(|id| inputs.turns.iter().find(|t| t.turn_id == id))
        .map_or(i64::MAX, |t| t.start_request_id);
    let items = genuine_stream(inputs)
        .into_iter()
        .filter(|r| r.request_id < before)
        .map(|r| operator_words(&r.text))
        .collect();
    list(
        "asks",
        "EARLIER OPERATOR MESSAGES (newest kept, word for word):",
        items,
        "operator messages are",
    )
}

fn corrections_section(inputs: &DossierInputs) -> AllocSection {
    let items = inputs
        .corrections
        .iter()
        .filter(|c| match &c.source {
            CorrectionSource::Label { tier, split, .. } => {
                matches!(tier.as_str(), "gold" | "relayed") && split == "dev"
            }
            CorrectionSource::LiveVerdict { .. } => true,
        })
        .map(|c| {
            format!(
                "Earlier ask: {}\nThe operator's correction: {}",
                cut(&operator_words(&c.corrected_prompt), CORR_ASK_CAP),
                cut(&operator_words(&c.correcting_text), CORR_TEXT_CAP)
            )
        })
        .collect();
    list(
        "corrections",
        "EARLIER CORRECTIONS (the operator's words):",
        items,
        "corrections are",
    )
}

fn semantic_lines(
    inputs: &DossierInputs,
    pick: impl Fn(
        &crate::semantic::sections::SemanticSections,
    ) -> Option<&crate::semantic::neighbours::NeighbourSet>,
    label: &str,
) -> Vec<String> {
    crate::semantic::sections::lines(inputs.semantic.as_ref().and_then(pick), label)
}

fn section_for(
    inputs: &DossierInputs,
    section: &SteererSection,
    turn_id: Option<i64>,
) -> AllocSection {
    match section {
        SteererSection::Ending => AllocSection {
            id: "ending",
            heading: "THE TURN'S ENDING (assistant text, sidecar requests removed; cut at the front when long):".to_owned(),
            body: SectionBody::Tail { text: ending_text(inputs) },
        },
        SteererSection::RecentReactions => AllocSection {
            id: "recent_reactions",
            heading: RECENT_REACTIONS_HEADING.to_owned(),
            body: SectionBody::NewestFirst {
                items: recent_reactions(inputs).iter().map(|r| reaction_block(r, inputs)).collect(),
                noun: "reactions are",
            },
        },
        SteererSection::Directives { classes } => directive_section(inputs, classes.as_ref()),
        SteererSection::Promises => list(
            "promises",
            "OPEN PROMISES AND WAITS (only turns whose ending matched a promise or wait phrase, about half of real stalls; registry items recorded as backing; wait backing itself is in the steering facts):",
            full_view(inputs)
                .promises
                .iter()
                .filter(|p| p.status_at == "open")
                .map(promise_line)
                .collect(),
            "promises are",
        ),
        SteererSection::SteeringFacts => list(
            "steering_facts",
            "STEERING FACTS FOR THE EVALUATED TURN (computed as of the moment, with evidence ids):",
            super::steerer_facts::fact_lines(inputs.steering_facts.as_ref()),
            "facts are",
        ),
        SteererSection::Asks => asks_section(inputs, turn_id),
        SteererSection::Corrections => corrections_section(inputs),
        SteererSection::SimilarCorrections => list("similar_corrections", "SIMILAR CORRECTIONS FROM OTHER SESSIONS:", semantic_lines(inputs, |s| s.corrections.as_ref(), "correction"), "corrections are"),
        SteererSection::SimilarDirectives => list("similar_directives", "SIMILAR DIRECTIVES FROM OTHER SESSIONS:", semantic_lines(inputs, |s| s.directives.as_ref(), "directive"), "directives are"),
        SteererSection::SimilarLessons => list("similar_lessons", "SIMILAR LESSONS:", semantic_lines(inputs, |s| s.lessons.as_ref(), "lesson"), "lessons are"),
        SteererSection::EarlierNotes => super::steerer_notes::earlier_notes_section(inputs),
        SteererSection::TurnOpening => super::steerer_notes::turn_opening_section(inputs, turn_id),
        SteererSection::EarlierTurns => super::steerer_notes::earlier_turns_section(inputs, turn_id),
    }
}

pub fn render_steerer(
    inputs: &DossierInputs,
    turn_id: Option<i64>,
    sections: &[SteererSection],
    budget: usize,
) -> SteererRender {
    let alloc_sections: Vec<AllocSection> = sections
        .iter()
        .map(|s| section_for(inputs, s, turn_id))
        .collect();
    let allocated = allocate(&alloc_sections, budget, &[]);
    SteererRender {
        view: SteererView {
            recent_reactions: recent_reactions(inputs),
            directives: directives_in_force(inputs),
            steering_facts: inputs.steering_facts.clone(),
            semantic: inputs.semantic.clone(),
            rendered: allocated.text.clone(),
        },
        allocated,
    }
}
