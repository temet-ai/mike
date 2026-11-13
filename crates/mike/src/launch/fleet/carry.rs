use crate::store::persistence::fleet_repo::FleetPersist as _;
use serde_json::Value;
use uuid::Uuid;

use crate::store::persistence::fleet::{CarryNote, CarryOutcome, NewCarry};
use crate::store::persistence::{PersistError, PersistHandle};

pub const TAG_PREFIX: &str = "carry:";
const KINDS: [&str; 4] = ["correction", "convention", "warning", "preference"];
const DEFAULT_KIND: &str = "convention";
const WARM_HEADING: &str =
    "Carried from earlier runs. Each line is a lesson the operator approved from a seat's finding:";
const LOAD_LIST_MARKER: &str = "Load these, in this order";

pub fn slug(title: &str) -> String {
    let words: Vec<String> = title
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    if words.is_empty() {
        "untitled".to_string()
    } else {
        words.join("-")
    }
}

pub fn carry_tag(title: &str) -> String {
    format!("{TAG_PREFIX}{}", slug(title))
}

pub fn marked(data: &Value) -> bool {
    data.get("carry") == Some(&Value::Bool(true))
}

pub fn carry_tags(tags: &[String]) -> Vec<String> {
    let mut found: Vec<String> = tags
        .iter()
        .filter(|t| t.starts_with(TAG_PREFIX) && t.len() > TAG_PREFIX.len())
        .cloned()
        .collect();
    found.sort();
    found.dedup();
    found
}

pub fn relevant(tags: &[String], data: &Value) -> bool {
    marked(data) || !carry_tags(tags).is_empty()
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn lesson_text(title: &str, data: &Value) -> String {
    let line = data
        .get("line")
        .and_then(Value::as_str)
        .map(one_line)
        .filter(|l| !l.is_empty());
    line.unwrap_or_else(|| one_line(title))
}

pub fn lesson_kind(data: &Value) -> &'static str {
    let wanted = data.get("kind").and_then(Value::as_str);
    KINDS
        .iter()
        .find(|k| Some(**k) == wanted)
        .copied()
        .unwrap_or(DEFAULT_KIND)
}

async fn discipline_of(
    persist: &PersistHandle,
    facts: &CarryNote,
) -> Result<Option<String>, PersistError> {
    let named = facts
        .data
        .get("discipline")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(str::to_string);
    if named.is_some() {
        return Ok(named);
    }
    if let Some(agent) = persist.fleet().carry_seat_agent(facts.session_uuid).await? {
        return Ok(Some(agent));
    }
    Ok(facts.role.clone())
}

async fn settle(
    persist: &PersistHandle,
    note_uuid: Uuid,
    tags: &[String],
    data: &Value,
) -> Result<CarryOutcome, PersistError> {
    let carries = marked(data);
    let mut wanted = carry_tags(tags);
    if !carries && wanted.is_empty() {
        return Ok(CarryOutcome::Skipped);
    }
    if carries
        && persist
            .fleet()
            .carry_own_candidate(note_uuid)
            .await?
            .is_some()
    {
        return Ok(CarryOutcome::AlreadyCandidate);
    }
    let Some(facts) = persist.fleet().carry_note_facts(note_uuid).await? else {
        return Ok(CarryOutcome::Skipped);
    };
    let discipline = if carries {
        discipline_of(persist, &facts).await?
    } else {
        None
    };
    let own_tag = carry_tag(&facts.title);
    if carries {
        wanted.push(own_tag.clone());
    }
    let counted = persist
        .fleet()
        .carry_count_findings((note_uuid, &facts), &wanted, discipline.as_deref())
        .await?;
    if counted > 0 {
        return Ok(CarryOutcome::Counted(counted));
    }
    let Some(discipline) = discipline else {
        return Ok(CarryOutcome::Skipped);
    };
    let text = lesson_text(&facts.title, &facts.data);
    let new = NewCarry {
        kind: lesson_kind(&facts.data),
        text: &text,
        discipline: &discipline,
        tag: &own_tag,
    };
    let created = persist
        .fleet()
        .carry_create_candidate((note_uuid, &facts), &new)
        .await?;
    Ok(created.map_or(CarryOutcome::Skipped, CarryOutcome::Candidate))
}

pub async fn after_note(
    persist: &PersistHandle,
    note_uuid: Uuid,
    tags: &[String],
    data: &Value,
) -> CarryOutcome {
    if !relevant(tags, data) {
        return CarryOutcome::Skipped;
    }
    match settle(persist, note_uuid, tags, data).await {
        Ok(outcome) => outcome,
        Err(err) => {
            tracing::warn!(error = %err, %note_uuid, "carry: a note could not be carried or counted");
            CarryOutcome::Skipped
        }
    }
}

pub fn warm_block(lines: &[String]) -> String {
    let items: Vec<String> = lines
        .iter()
        .map(|l| one_line(l))
        .filter(|l| !l.is_empty())
        .map(|l| format!("- {l}"))
        .collect();
    if items.is_empty() {
        return String::new();
    }
    format!("{WARM_HEADING}\n{}", items.join("\n"))
}

pub fn with_carry(prompt: &str, lines: &[String]) -> String {
    let block = warm_block(lines);
    if block.is_empty() {
        return prompt.to_string();
    }
    match prompt.find(LOAD_LIST_MARKER) {
        Some(at) => {
            let (head, tail) = prompt.split_at(at);
            format!("{}\n\n{block}\n\n{tail}", head.trim_end())
        }
        None => format!("{prompt}\n\n{block}"),
    }
}

pub async fn warm_lines(
    persist: &PersistHandle,
    fleet: &str,
    run_id: &str,
    disciplines: &[&str],
) -> Vec<String> {
    let Ok(fleet) = fleet.parse::<Uuid>() else {
        return Vec::new();
    };
    let names: Vec<String> = disciplines.iter().map(|d| d.to_string()).collect();
    match persist
        .fleet()
        .carried_for_warmup(fleet, run_id, &names)
        .await
    {
        Ok(lines) => lines,
        Err(err) => {
            tracing::warn!(error = %err, run = %run_id, "carry: the carried lessons could not be read for a template");
            Vec::new()
        }
    }
}
