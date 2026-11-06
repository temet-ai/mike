use std::collections::HashSet;

use super::{AuthoredReference, AuthoredSkill};

pub const MAX_DESCRIPTION_CHARS: usize = 1_024;
pub const MAX_OVERVIEW_CHARS: usize = 6_000;
pub const MAX_REFERENCE_CHARS: usize = 20_000;
pub const MAX_REFERENCES: usize = 6;
const RESERVED_REFERENCE: &str = "lessons";

pub fn normalise_reference_names(authored: &mut AuthoredSkill) {
    for reference in &mut authored.references {
        let name = reference.name.trim();
        let name = name.strip_prefix("references/").unwrap_or(name);
        let name = name.strip_suffix(".md").unwrap_or(name);
        reference.name = name.to_owned();
    }
}

pub fn acceptable(authored: &AuthoredSkill) -> Result<(), String> {
    within("description", &authored.description, MAX_DESCRIPTION_CHARS)?;
    within("overview", &authored.overview, MAX_OVERVIEW_CHARS)?;
    references(authored)?;
    evaluation(authored)
}

fn non_empty(field: &str, text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err(format!("empty {field}"));
    }
    Ok(())
}

fn within(field: &str, text: &str, limit: usize) -> Result<(), String> {
    non_empty(field, text)?;
    let chars = text.trim().chars().count();
    if chars > limit {
        return Err(format!("{field} is {chars} characters, limit {limit}"));
    }
    Ok(())
}

fn references(authored: &AuthoredSkill) -> Result<(), String> {
    let count = authored.references.len();
    if count == 0 || count > MAX_REFERENCES {
        return Err(format!(
            "{count} reference files, need 1 to {MAX_REFERENCES}"
        ));
    }
    let mut seen = HashSet::new();
    for reference in &authored.references {
        reference_shape(reference)?;
        if !seen.insert(reference.name.as_str()) {
            return Err(format!("reference {} named twice", reference.name));
        }
        pointed_to(&authored.overview, &reference.name)?;
    }
    Ok(())
}

fn reference_shape(reference: &AuthoredReference) -> Result<(), String> {
    if !is_file_name(&reference.name) || reference.name == RESERVED_REFERENCE {
        return Err(format!(
            "reference name {:?} is not allowed",
            reference.name
        ));
    }
    non_empty("when_to_read", &reference.when_to_read)?;
    within(&reference.name, &reference.content, MAX_REFERENCE_CHARS)
}

fn is_file_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.split('-').all(|word| {
            !word.is_empty()
                && word
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

fn pointed_to(overview: &str, name: &str) -> Result<(), String> {
    if overview.contains(&format!("references/{name}.md")) {
        return Ok(());
    }
    Err(format!("overview does not point to references/{name}.md"))
}

fn evaluation(authored: &AuthoredSkill) -> Result<(), String> {
    let yes = authored
        .trigger_queries
        .iter()
        .filter(|q| q.should_trigger)
        .count();
    let no = authored.trigger_queries.len() - yes;
    if yes < 6 || no < 6 {
        return Err(format!(
            "trigger queries: {yes} should and {no} should-not, need 6 of each"
        ));
    }
    Ok(())
}
