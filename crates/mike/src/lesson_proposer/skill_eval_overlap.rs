use std::collections::HashSet;

use super::progress::Progress;
use crate::store::persistence::lessons::skill_draft_eval::AgentScope;

const SLUG_LIMIT: usize = 40;

fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let kept: String = out.trim_end_matches('-').chars().take(SLUG_LIMIT).collect();
    kept.trim_end_matches('-').to_owned()
}

pub(super) fn own_folders(folder: &str, agent: Option<&AgentScope>) -> Vec<String> {
    let mut own = vec![folder.to_owned()];
    let suffix = agent.map(|a| slug(&a.name)).filter(|s| !s.is_empty());
    let topic = suffix
        .and_then(|s| folder.strip_suffix(format!("-{s}").as_str()))
        .filter(|topic| !topic.is_empty());
    own.extend(topic.map(str::to_owned));
    own
}

pub(super) fn existing_skills(progress: &Progress, own: &[String]) -> Vec<(String, String)> {
    let siblings = progress.siblings.as_deref().unwrap_or_default();
    let real = progress
        .real_skills
        .as_ref()
        .map(|r| r.listed.as_slice())
        .unwrap_or_default();
    let mut seen: HashSet<&str> = own.iter().map(String::as_str).collect();
    siblings
        .iter()
        .chain(real)
        .filter(|skill| seen.insert(skill.folder.as_str()))
        .map(|skill| (skill.folder.clone(), skill.description.clone()))
        .collect()
}
