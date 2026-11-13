use std::fmt::Write as _;

use super::Question;
use super::gates::tail_chars;
use super::step_class::DirectiveInForce;
use crate::dossier::steering_facts::SteeringFacts;

pub const ENDING_CHARS: usize = 3_000;
pub const TRAJECTORY_CHARS: usize = 8_000;

#[derive(Debug, Clone, Default)]
pub struct InputParts<'a> {
    pub ending: &'a str,
    pub open: Vec<Question>,
    pub correction: Option<(i64, String, String)>,
    pub correction_rationale: Option<String>,
    pub directives: &'a [DirectiveInForce],
    pub asked_class: Option<&'static str>,
    pub facts: Option<&'a SteeringFacts>,
    pub trajectory: Option<&'a str>,
    pub goal: Option<&'a str>,
    pub dossier: Option<&'a str>,
}

fn whole_lines_tail(text: &str, n: usize) -> &str {
    let tail = tail_chars(text, n);
    if tail.len() == text.len() {
        return tail;
    }
    tail.find('\n').map_or(tail, |i| &tail[i + 1..])
}

fn push_section(out: &mut String, title: &str, body: &str) {
    let _ = write!(out, "## {title}\n{body}\n\n");
}

fn facts_block(facts: Option<&SteeringFacts>) -> String {
    let Some(f) = facts else {
        return "no steering facts could be read".to_owned();
    };
    let refs: Vec<String> = f
        .wait_backing_refs
        .iter()
        .map(|r| {
            format!(
                "{} {} ({}{})",
                r.source,
                r.id.unwrap_or_default(),
                r.kind.clone().unwrap_or_default(),
                r.state
                    .as_ref()
                    .map(|s| format!(", {s}"))
                    .unwrap_or_default()
            )
        })
        .collect();
    format!(
        "wait backing: {} [{}]\nsession class: {}\n",
        f.wait_backing.code(),
        if refs.is_empty() {
            "nothing running, awaited or scheduled".to_owned()
        } else {
            refs.join("; ")
        },
        f.session_class.as_ref().map_or("unknown", |c| c.class),
    )
}

fn observation_block(facts: Option<&SteeringFacts>) -> String {
    let Some(o) = facts.map(|f| &f.observation) else {
        return "unknown".to_owned();
    };
    format!(
        "last change: {} at {}\nobserved after change: {}\nlast observation: {} ({}) at {}\n",
        o.last_change_use_id
            .map_or("none".to_owned(), |id| format!("tool {id}")),
        o.last_change_at.map_or("-".to_owned(), |t| t.to_rfc3339()),
        o.observed_after_change
            .map_or("no change".to_owned(), |b| b.to_string()),
        o.last_observation_use_id
            .map_or("none".to_owned(), |id| format!("tool {id}")),
        o.observation_kind.unwrap_or("-"),
        o.last_observation_at
            .map_or("-".to_owned(), |t| t.to_rfc3339()),
    )
}

pub fn build_input(parts: &InputParts<'_>) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Questions: {}\n",
        parts
            .open
            .iter()
            .map(|q| q.code())
            .collect::<Vec<_>>()
            .join(", ")
    );
    push_section(
        &mut out,
        "ENDING",
        whole_lines_tail(parts.ending, ENDING_CHARS),
    );
    push_section(&mut out, "FACTS", &facts_block(parts.facts));
    if parts.open.contains(&Question::Relapse)
        && let Some((turn_id, at, text)) = &parts.correction
    {
        let rationale = parts.correction_rationale.as_deref().unwrap_or("");
        push_section(
            &mut out,
            "CORRECTION",
            &format!("turn {turn_id} at {at}\n{text}\nreaction verdict: {rationale}"),
        );
    }
    if parts.open.contains(&Question::AuthorisedHandback) {
        let lines: Vec<String> = parts
            .directives
            .iter()
            .filter(|d| {
                parts
                    .asked_class
                    .is_none_or(|c| d.class == c || d.class == "proceed" || d.class == "ask_first")
            })
            .map(|d| {
                format!(
                    "directive {} ({} {}, turn {}): {}",
                    d.id,
                    d.polarity,
                    d.class,
                    d.operator_turn_id.unwrap_or_default(),
                    d.text
                )
            })
            .collect();
        push_section(
            &mut out,
            "DIRECTIVES",
            &if lines.is_empty() {
                "none in force".to_owned()
            } else {
                lines.join("\n")
            },
        );
    }
    if parts.open.contains(&Question::UnobservedDone) {
        push_section(
            &mut out,
            "CHANGE AND OBSERVATION",
            &observation_block(parts.facts),
        );
        if let Some(t) = parts.trajectory {
            push_section(&mut out, "TRAJECTORY", tail_chars(t, TRAJECTORY_CHARS));
        }
    }
    if parts.open.contains(&Question::UnbackedWait) {
        push_section(&mut out, "WAIT BACKING", &facts_block(parts.facts));
    }
    if let Some(goal) = parts.goal {
        push_section(&mut out, "SESSION GOAL", goal);
    }
    if let Some(d) = parts.dossier {
        push_section(&mut out, "DOSSIER", d);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_input_carries_only_open_question_sections() {
        let parts = InputParts {
            ending: "line one\nI'll report back when the build finishes.",
            open: vec![Question::UnbackedWait],
            ..InputParts::default()
        };
        let text = build_input(&parts);
        assert!(text.contains("## ENDING"));
        assert!(text.contains("## WAIT BACKING"));
        assert!(!text.contains("## DIRECTIVES"));
        assert!(!text.contains("## CORRECTION"));
    }
}
