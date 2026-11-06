use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::store::persistence::lessons::skill_draft_eval::{
    ListingBody, ListingWindow, SiblingSkill,
};
use crate::store::persistence::{PersistError, PersistHandle};

pub(super) const LISTING_MARKER: &str = "available for use with the Skill tool:";
pub(super) const SAMPLE_SIZE: usize = 60;
const WINDOW: ListingWindow = ListingWindow {
    candidates: 24,
    sessions: 5,
    min_bytes: 20_000,
    max_bytes: 600_000,
};

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct RealSkills {
    pub request_ids: Vec<i64>,
    pub seen: usize,
    pub skills: Vec<SiblingSkill>,
    #[serde(default)]
    pub listed: Vec<SiblingSkill>,
}

fn entry(line: &str) -> Option<SiblingSkill> {
    let body = line.strip_prefix("- ")?;
    let (name, description) = body.split_once(": ")?;
    let name = name.trim();
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    Some(SiblingSkill {
        folder: name.to_owned(),
        description: description.trim().to_owned(),
    })
}

fn listing_in(text: &str) -> Vec<SiblingSkill> {
    let Some(at) = text.find(LISTING_MARKER) else {
        return Vec::new();
    };
    let rest = text[at + LISTING_MARKER.len()..].trim_start_matches(['\r', '\n']);
    let mut skills: Vec<SiblingSkill> = Vec::new();
    for line in rest.lines() {
        if line.trim().is_empty() {
            break;
        }
        match entry(line) {
            Some(skill) => skills.push(skill),
            None => {
                if let Some(last) = skills.last_mut() {
                    last.description.push(' ');
                    last.description.push_str(line.trim());
                }
            }
        }
    }
    skills
}

fn texts_of<'a>(value: &'a Value, out: &mut Vec<&'a str>) {
    match value {
        Value::String(text) => out.push(text),
        Value::Array(items) => items.iter().for_each(|v| texts_of(v, out)),
        Value::Object(map) => map.values().for_each(|v| texts_of(v, out)),
        _ => {}
    }
}

pub(super) fn parse_listing(body: &[u8]) -> Vec<SiblingSkill> {
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return Vec::new();
    };
    for key in ["system", "messages"] {
        let mut texts = Vec::new();
        if let Some(part) = value.get(key) {
            texts_of(part, &mut texts);
        }
        if let Some(found) = texts
            .into_iter()
            .map(listing_in)
            .find(|skills| !skills.is_empty())
        {
            return found;
        }
    }
    Vec::new()
}

fn rank(seed: i64, name: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64 ^ seed.unsigned_abs();
    for byte in name.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

fn sample(mut skills: Vec<SiblingSkill>, size: usize, seed: i64) -> Vec<SiblingSkill> {
    if skills.len() <= size {
        return skills;
    }
    skills.sort_by_key(|s| rank(seed, &s.folder));
    skills.into_iter().take(size).collect()
}

pub(super) fn gather(bodies: &[ListingBody], own_name: &str, seed: i64) -> RealSkills {
    let mut names: HashSet<String> = HashSet::new();
    names.insert(own_name.to_owned());
    let mut request_ids = Vec::new();
    let mut union = Vec::new();
    for body in bodies {
        let listed = parse_listing(&body.body);
        if listed.is_empty() {
            continue;
        }
        request_ids.push(body.request_id);
        for skill in listed {
            if names.insert(skill.folder.clone()) {
                union.push(skill);
            }
        }
    }
    RealSkills {
        request_ids,
        seen: union.len(),
        listed: union.clone(),
        skills: sample(union, SAMPLE_SIZE, seed),
    }
}

pub(super) async fn load(
    persist: &PersistHandle,
    project_id: i64,
    own_name: &str,
    seed: i64,
) -> Result<RealSkills, PersistError> {
    let bodies = persist
        .recent_skill_listing_bodies(project_id, LISTING_MARKER.as_bytes(), WINDOW)
        .await?;
    Ok(gather(&bodies, own_name, seed))
}
