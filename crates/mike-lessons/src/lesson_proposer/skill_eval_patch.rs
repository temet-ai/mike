use super::Material;
use crate::lesson_proposer::skill_author::{MARKER, MAX_OVERVIEW_CHARS};

pub const MAX_SKILL_MD_CHARS: usize = MAX_OVERVIEW_CHARS * 2;
const LESSONS_REFERENCE: &str = "lessons";

pub struct Patched {
    pub description: String,
    pub skill_md: String,
}

fn frontmatter(text: &str) -> Option<(&str, &str)> {
    let rest = text.trim_start().strip_prefix("---")?.strip_prefix('\n')?;
    let end = rest.find("\n---")?;
    let after = &rest[end + 4..];
    if !(after.is_empty() || after.starts_with('\n') || after.starts_with('\r')) {
        return None;
    }
    Some((&rest[..end], after))
}

fn field<'a>(front: &'a str, key: &str) -> Option<&'a str> {
    let prefix = format!("{key}:");
    front
        .lines()
        .find_map(|line| line.strip_prefix(prefix.as_str()))
        .map(str::trim)
}

fn description_of(value: &str) -> Result<String, String> {
    let text = if value.starts_with('"') {
        serde_json::from_str::<String>(value)
            .map_err(|e| format!("description is not a valid quoted string: {e}"))?
    } else {
        value.trim_matches('\'').to_owned()
    };
    if text.trim().is_empty() || text.starts_with(['>', '|']) {
        return Err("description must be one non-empty line".to_owned());
    }
    Ok(text.trim().to_owned())
}

fn linked_references(body: &str) -> Vec<&str> {
    body.match_indices("references/")
        .filter_map(|(at, marker)| {
            let rest = &body[at + marker.len()..];
            let end = rest
                .find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'))
                .unwrap_or(rest.len());
            rest[end..].starts_with(".md").then(|| &rest[..end])
        })
        .collect()
}

pub fn patch(material: &Material, skill_md: &str) -> Result<Patched, String> {
    let chars = skill_md.chars().count();
    if chars > MAX_SKILL_MD_CHARS {
        return Err(format!(
            "SKILL.md is {chars} characters, limit {MAX_SKILL_MD_CHARS}"
        ));
    }
    let (front, body) = frontmatter(skill_md).ok_or("SKILL.md has no frontmatter")?;
    if field(front, "name") != Some(material.name.as_str()) {
        return Err(format!("name must stay {}", material.name));
    }
    let description = description_of(field(front, "description").ok_or("no description")?)?;
    if !body.contains(MARKER) {
        return Err("the marker line is missing".to_owned());
    }
    let linked = linked_references(body);
    for reference in &material.references {
        if !linked.contains(&reference.name.as_str()) {
            return Err(format!("no pointer to references/{}.md", reference.name));
        }
    }
    let known = |name: &str| {
        name == LESSONS_REFERENCE || material.references.iter().any(|r| r.name == name)
    };
    if let Some(unknown) = linked.iter().find(|name| !known(name)) {
        return Err(format!(
            "points to references/{unknown}.md, which does not exist"
        ));
    }
    Ok(Patched {
        description,
        skill_md: skill_md.trim().to_owned() + "\n",
    })
}
