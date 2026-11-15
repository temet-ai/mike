use std::collections::BTreeMap;

use serde_json::Value;

use super::promotion::PromotionState;
use super::rows::{int, non_empty, percent, rows, text};

const DEFAULT_SKILL_CAP: i64 = 12;

fn unavailable(title: &str, reason: &str) -> Vec<String> {
    vec![
        String::new(),
        format!("## {title}"),
        String::new(),
        reason.to_owned(),
    ]
}

fn shape_summary(shapes: &[Value], injected: &BTreeMap<String, &Value>) -> Vec<String> {
    let active: i64 = shapes.iter().map(|row| int(row, "lessons")).sum();
    let chars: i64 = shapes.iter().map(|row| int(row, "stored_chars")).sum();
    let injected_chars: i64 = injected
        .values()
        .map(|row| int(row, "injected_chars"))
        .sum();
    let mean = if active == 0 {
        0
    } else {
        (chars as f64 / active as f64).round_ties_even() as i64
    };
    let mut lines = vec![
        format!(
            "- Active lessons: {active}; mean length {mean} characters; {} over 600 characters; {} under 80 characters.",
            shapes.iter().map(|row| int(row, "over_600_chars")).sum::<i64>(),
            shapes.iter().map(|row| int(row, "under_80_chars")).sum::<i64>(),
        ),
        String::new(),
        "| Shape | Active lessons | Stored characters | Injections | Injected characters | Share of injected characters |".to_owned(),
        "|---|---:|---:|---:|---:|---:|".to_owned(),
    ];
    let none = Value::Null;
    for row in shapes {
        let sent = injected.get(&text(row, "shape")).copied().unwrap_or(&none);
        let share = if injected_chars == 0 {
            0.0
        } else {
            int(sent, "injected_chars") as f64 / injected_chars as f64
        };
        lines.push(format!(
            "| {} | {} | {} | {} | {} | {} |",
            text(row, "shape"),
            int(row, "lessons"),
            int(row, "stored_chars"),
            int(sent, "injections"),
            int(sent, "injected_chars"),
            percent(share),
        ));
    }
    lines
}

fn quality_follow_ups(data: &Value) -> Vec<String> {
    let mut lines = Vec::new();
    for row in rows(data, "lesson_shape_pending") {
        lines.push(format!(
            "- Waiting for a rewrite: {} `{}` lessons, oldest found {}.",
            int(row, "lessons"),
            text(row, "shape"),
            text(row, "oldest_found_at"),
        ));
    }
    let resolved: Vec<String> = rows(data, "lesson_shape_resolved")
        .iter()
        .map(|row| format!("{} {}", int(row, "lessons"), text(row, "resolution")))
        .collect();
    let resolved = if resolved.is_empty() {
        "none".to_owned()
    } else {
        resolved.join(", ")
    };
    lines.push(format!("- Resolved this window: {resolved}."));
    for row in rows(data, "lesson_relations_write_time") {
        lines.push(format!(
            "- Relations `{}` `{}` `{}`: {} ({} applied by consolidation).",
            text(row, "method"),
            text(row, "kind"),
            text(row, "status"),
            int(row, "relations"),
            int(row, "applied"),
        ));
    }
    lines
}

fn replacement_effect(data: &Value) -> Vec<String> {
    let effect: BTreeMap<String, &Value> = rows(data, "lesson_replacement_effect")
        .iter()
        .map(|row| (text(row, "side"), row))
        .collect();
    ["replaced", "replacement"]
        .into_iter()
        .filter_map(|side| {
            let row = effect.get(side)?;
            let turns = int(row, "turns");
            let corrected = int(row, "corrected_turns");
            let rate = if turns == 0 {
                0.0
            } else {
                corrected as f64 / turns as f64
            };
            Some(format!(
                "- Turns that received the {side} lesson: {turns}, {corrected} corrected ({}). A rate difference over a few turns is a reason to read the pair, not evidence of harm.",
                percent(rate)
            ))
        })
        .collect()
}

pub(super) fn render_lesson_quality(data: &Value) -> Vec<String> {
    let shapes = match data.get("lesson_shape") {
        None => return Vec::new(),
        Some(Value::Null) => {
            return unavailable(
                "Lesson quality",
                "Not available: the database does not have the lesson shape table yet, because the migration 20261014190000_lesson_shape_findings has not run. Start the proxy to apply it.",
            );
        }
        Some(Value::Array(shapes)) => shapes,
        Some(_) => return Vec::new(),
    };
    let injected: BTreeMap<String, &Value> = rows(data, "lesson_shape_injected")
        .iter()
        .map(|row| (text(row, "shape"), row))
        .collect();
    let mut lines = vec![
        String::new(),
        "## Lesson quality".to_owned(),
        String::new(),
        concat!(
            "A rule is a durable instruction with its trigger and what to do. A status lesson reports on a session. ",
            "An over-length lesson exceeds the `lessons.shape.max_chars` setting. \"Unchecked\" lessons have not been ",
            "reached by the shape check yet. Counts cover active daily and long-term lessons; injected characters ",
            "cover every injection in the window, including lessons archived since."
        )
        .to_owned(),
        String::new(),
    ];
    lines.extend(shape_summary(shapes, &injected));
    lines.extend(quality_follow_ups(data));
    lines.extend(replacement_effect(data));
    lines
}

fn skill_cap(data: &Value) -> i64 {
    rows(data, "skill_cap_setting")
        .first()
        .and_then(|row| row.get("value"))
        .and_then(Value::as_i64)
        .unwrap_or(DEFAULT_SKILL_CAP)
}

fn skills_by_project(data: &Value, promotion: &PromotionState) -> Vec<(String, usize)> {
    let names: BTreeMap<String, String> = rows(data, "project_names")
        .iter()
        .map(|row| (text(row, "project_uuid"), text(row, "project_path")))
        .collect();
    let mut skills: Vec<(String, usize)> = promotion
        .live_skills_per_project()
        .into_iter()
        .map(|(uuid, count)| (names.get(&uuid).cloned().unwrap_or(uuid), count))
        .collect();
    skills.sort();
    skills
}

fn skill_cap_lines(data: &Value, promotion: &PromotionState) -> Vec<String> {
    let cap = skill_cap(data);
    let skills = skills_by_project(data, promotion);
    let mut lines = vec![
        String::new(),
        format!(
            "Mike-written skills against the cap of {cap} per project (setting `lessons.skills.max_per_project`; counted from this machine's promotion ledger):"
        ),
        String::new(),
    ];
    if skills.is_empty() {
        lines.push("- No mike-written skill is shipped on this machine.".to_owned());
        return lines;
    }
    lines.push("| Project | Skills | Cap | State |".to_owned());
    lines.push("|---|---:|---:|---|".to_owned());
    for (name, count) in skills {
        let state = if count as i64 > cap {
            "over the cap"
        } else {
            "within the cap"
        };
        lines.push(format!("| {name} | {count} | {cap} | {state} |"));
    }
    lines
}

fn proposal_lines(data: &Value) -> Vec<String> {
    let mut lines = Vec::new();
    for row in rows(data, "lesson_placement_proposals") {
        let scope = non_empty(row, "agent")
            .map(|agent| format!(", agent {agent}"))
            .unwrap_or_default();
        let covered = non_empty(row, "covered_by")
            .map(|document| format!(" Covered by or belongs in: {document}."))
            .unwrap_or_default();
        let reason = non_empty(row, "reason").unwrap_or("no reason recorded");
        lines.extend([
            String::new(),
            format!(
                "### Proposed {} entry: lesson {} ({}{scope})",
                text(row, "placement"),
                text(row, "id"),
                text(row, "project"),
            ),
            String::new(),
            text(row, "text"),
            String::new(),
            format!("Why: {reason}.{covered}"),
        ]);
    }
    lines
}

pub(super) fn render_placement(data: &Value, promotion: &PromotionState) -> Vec<String> {
    let placed = match data.get("lesson_placement") {
        None => return Vec::new(),
        Some(Value::Null) => {
            return unavailable(
                "Where lessons belong",
                "Not available: the database does not have the lesson placement columns yet, because the migration 20261014200000_lesson_placement has not run. Start the proxy to apply it.",
            );
        }
        Some(Value::Array(placed)) => placed,
        Some(_) => return Vec::new(),
    };
    let total: i64 = placed.iter().map(|row| int(row, "lessons")).sum();
    let stuck = rows(data, "lesson_placement_stuck")
        .first()
        .map_or(0, |row| int(row, "lessons"));
    let mut lines = vec![
        String::new(),
        "## Where lessons belong".to_owned(),
        String::new(),
        concat!(
            "Each active lesson has one home: a rule (a short constraint tied to files, proposed to the operator), ",
            "a skill (a procedure for a recognisable situation; only these feed skill drafts), an agent definition ",
            "(a persona, proposed to the operator), memory (a fact or correction recalled when relevant), or nowhere ",
            "(already covered, or not worth keeping). \"Unplaced\" lessons have not been reached by the placement pass yet."
        )
        .to_owned(),
        String::new(),
        "| Placement | Active lessons | Agent-scoped | Naming a covering document |".to_owned(),
        "|---|---:|---:|---:|".to_owned(),
    ];
    lines.extend(placed.iter().map(|row| {
        format!(
            "| {} | {} | {} | {} |",
            text(row, "placement"),
            int(row, "lessons"),
            int(row, "agent_scoped"),
            int(row, "naming_a_document"),
        )
    }));
    lines.push(format!(
        "- Active lessons: {total}; {stuck} could not be placed after three attempts."
    ));
    lines.extend(skill_cap_lines(data, promotion));
    lines.extend(proposal_lines(data));
    lines
}
