use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Deserialize)]
pub struct RawStatement {
    pub text: String,
    pub passage: String,
    #[serde(default)]
    pub about: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SplitAnswer {
    pub statements: Vec<RawStatement>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberRole {
    Keep,
    Covered,
    Adds,
    Conflicts,
    Elsewhere,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PullRole {
    Covered,
    Covers,
    Adds,
    Conflicts,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Member {
    pub id: i64,
    pub role: MemberRole,
    #[serde(default)]
    pub by: Option<i64>,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub evidence_for: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Pull {
    pub id: i64,
    pub role: PullRole,
    #[serde(default)]
    pub by: Option<i64>,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub evidence_for: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct EvidenceUse {
    pub id: i64,
    #[serde(default)]
    pub shows: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MergeGroup {
    pub ids: Vec<i64>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Outdated {
    pub id: i64,
    pub evidence_id: i64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Tighten {
    pub id: i64,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JudgeAnswer {
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub members: Vec<Member>,
    #[serde(default)]
    pub pull_in: Vec<Pull>,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub evidence_used: Vec<EvidenceUse>,
    #[serde(default)]
    pub merges: Vec<MergeGroup>,
    #[serde(default)]
    pub outdated: Vec<Outdated>,
    #[serde(default)]
    pub tighten: Vec<Tighten>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MissingAnswer {
    pub missing: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    Keep,
    CoveredBy(i64),
    AddsTo(i64),
    ConflictsWith(i64),
    Elsewhere,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidMember {
    pub id: i64,
    pub relation: Relation,
    pub note: String,
    pub evidence_for: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullKind {
    CoveredByMember,
    CoversMember,
    Adds,
    Conflicts,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidPull {
    pub id: i64,
    pub kind: PullKind,
    pub by: i64,
    pub note: String,
    pub evidence_for: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Validated {
    pub subject: String,
    pub text: String,
    pub members: Vec<ValidMember>,
    pub pulled: Vec<ValidPull>,
    pub merges: Vec<MergeGroup>,
    pub evidence_used: Vec<EvidenceUse>,
    pub outdated: Vec<Outdated>,
    pub tighten: Vec<Tighten>,
}

pub struct Universe<'a> {
    pub members: &'a [i64],
    pub nearby: &'a [i64],
    pub evidence: &'a [i64],
}

pub fn split_schema() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["statements"],"properties":{
        "statements":{"type":"array","items":{"type":"object","additionalProperties":false,
            "required":["text","passage","about"],
            "properties":{"text":{"type":"string"},"passage":{"type":"string"},"about":{"type":"string"}}}}}})
}

pub fn judge_schema() -> Value {
    let role = |roles: &[&str], by: Value| {
        json!({"type":"object","additionalProperties":false,
            "required":["id","role","by","note","evidence_for"],
            "properties":{"id":{"type":"integer"},"role":{"type":"string","enum":roles},
                "by":by,"note":{"type":"string"},"evidence_for":{"type":["integer","null"]}}})
    };
    json!({"type":"object","additionalProperties":false,
        "required":["subject","members","pull_in","text","evidence_used","merges","outdated","tighten"],
        "properties":{
            "subject":{"type":"string"},
            "members":{"type":"array","items":role(
                &["keep","covered","adds","conflicts","elsewhere"],json!({"type":["integer","null"]}))},
            "pull_in":{"type":"array","items":role(
                &["covered","adds","conflicts","covers"],json!({"type":"integer"}))},
            "text":{"type":"string"},
            "evidence_used":{"type":"array","items":{"type":"object","additionalProperties":false,
                "required":["id","shows"],"properties":{"id":{"type":"integer"},"shows":{"type":"string"}}}},
            "merges":{"type":"array","items":{"type":"object","additionalProperties":false,
                "required":["ids","text"],
                "properties":{"ids":{"type":"array","items":{"type":"integer"}},"text":{"type":"string"}}}},
            "outdated":{"type":"array","items":{"type":"object","additionalProperties":false,
                "required":["id","evidence_id","text"],
                "properties":{"id":{"type":"integer"},"evidence_id":{"type":"integer"},"text":{"type":"string"}}}},
            "tighten":{"type":"array","items":{"type":"object","additionalProperties":false,
                "required":["id","text"],
                "properties":{"id":{"type":"integer"},"text":{"type":"string"}}}}}})
}

pub fn missing_schema() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["missing"],
        "properties":{"missing":{"type":"array","items":{"type":"string"}}}})
}

struct Edges {
    covered_by: HashMap<i64, i64>,
}

impl Edges {
    fn reaches(&self, from: i64, target: i64) -> bool {
        let mut seen = HashSet::new();
        let mut at = from;
        while seen.insert(at) {
            if at == target {
                return true;
            }
            match self.covered_by.get(&at) {
                Some(next) => at = *next,
                None => return false,
            }
        }
        false
    }

    fn cover(&mut self, id: i64, by: i64) -> bool {
        if id == by || self.reaches(by, id) || self.covered_by.contains_key(&id) {
            return false;
        }
        self.covered_by.insert(id, by);
        true
    }
}

fn known_evidence_for(evidence_for: Option<i64>, a: i64, b: i64) -> Option<i64> {
    evidence_for.filter(|id| *id == a || *id == b)
}

fn valid_pulls(answer: &JudgeAnswer, universe: &Universe<'_>, edges: &mut Edges) -> Vec<ValidPull> {
    let members: HashSet<i64> = universe.members.iter().copied().collect();
    let nearby: HashSet<i64> = universe.nearby.iter().copied().collect();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for pull in &answer.pull_in {
        let Some(by) = pull.by.filter(|by| members.contains(by)) else {
            continue;
        };
        if !nearby.contains(&pull.id) || members.contains(&pull.id) || !seen.insert(pull.id) {
            continue;
        }
        let kind = match pull.role {
            PullRole::Covered if edges.cover(pull.id, by) => PullKind::CoveredByMember,
            PullRole::Covers if edges.cover(by, pull.id) => PullKind::CoversMember,
            PullRole::Adds => PullKind::Adds,
            PullRole::Conflicts => PullKind::Conflicts,
            _ => continue,
        };
        out.push(ValidPull {
            id: pull.id,
            kind,
            by,
            note: pull.note.clone(),
            evidence_for: known_evidence_for(pull.evidence_for, pull.id, by),
        });
    }
    out
}

fn member_relation(member: &Member, by: Option<i64>, edges: &mut Edges) -> Relation {
    match (member.role, by) {
        (MemberRole::Covered, Some(by)) if edges.cover(member.id, by) => Relation::CoveredBy(by),
        (MemberRole::Adds, Some(by)) => Relation::AddsTo(by),
        (MemberRole::Conflicts, Some(by)) => Relation::ConflictsWith(by),
        (MemberRole::Elsewhere, _) => Relation::Elsewhere,
        _ => Relation::Keep,
    }
}

fn valid_members(
    answer: &JudgeAnswer,
    universe: &Universe<'_>,
    pulled: &[ValidPull],
    edges: &mut Edges,
) -> Vec<ValidMember> {
    let members: HashSet<i64> = universe.members.iter().copied().collect();
    let pulled_ids: HashSet<i64> = pulled.iter().map(|p| p.id).collect();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for member in &answer.members {
        if !members.contains(&member.id) || !seen.insert(member.id) {
            continue;
        }
        let by = member
            .by
            .filter(|by| *by != member.id && (members.contains(by) || pulled_ids.contains(by)));
        let relation = match member_relation(member, by, edges) {
            Relation::Keep => edges
                .covered_by
                .get(&member.id)
                .map_or(Relation::Keep, |by| Relation::CoveredBy(*by)),
            other => other,
        };
        out.push(ValidMember {
            id: member.id,
            relation,
            note: member.note.clone(),
            evidence_for: by.and_then(|by| known_evidence_for(member.evidence_for, member.id, by)),
        });
    }
    for id in universe.members {
        if seen.insert(*id) {
            let relation = edges
                .covered_by
                .get(id)
                .map_or(Relation::Keep, |by| Relation::CoveredBy(*by));
            out.push(ValidMember {
                id: *id,
                relation,
                note: String::new(),
                evidence_for: None,
            });
        }
    }
    out
}

pub fn validate(answer: &JudgeAnswer, universe: &Universe<'_>) -> Validated {
    let mut edges = Edges {
        covered_by: HashMap::new(),
    };
    let pulled = valid_pulls(answer, universe, &mut edges);
    let members = valid_members(answer, universe, &pulled, &mut edges);
    let known: HashSet<i64> = universe
        .members
        .iter()
        .chain(universe.nearby)
        .copied()
        .collect();
    let evidence: HashSet<i64> = universe.evidence.iter().copied().collect();
    let outdated = valid_outdated(answer, &known, &evidence);
    let tighten = valid_tighten(answer, &known, &outdated);
    let mut evidence_used: Vec<EvidenceUse> = answer
        .evidence_used
        .iter()
        .filter(|used| evidence.contains(&used.id))
        .cloned()
        .collect();
    for item in &outdated {
        if !evidence_used.iter().any(|u| u.id == item.evidence_id) {
            evidence_used.push(EvidenceUse {
                id: item.evidence_id,
                shows: "corrects this statement".to_owned(),
            });
        }
    }
    Validated {
        outdated,
        tighten,
        subject: answer.subject.trim().to_owned(),
        text: answer.text.trim().to_owned(),
        members,
        pulled,
        merges: answer
            .merges
            .iter()
            .map(|group| MergeGroup {
                ids: group
                    .ids
                    .iter()
                    .copied()
                    .filter(|id| known.contains(id))
                    .collect(),
                text: group.text.trim().to_owned(),
            })
            .collect(),
        evidence_used,
    }
}

fn valid_outdated(
    answer: &JudgeAnswer,
    known: &HashSet<i64>,
    evidence: &HashSet<i64>,
) -> Vec<Outdated> {
    let mut seen = HashSet::new();
    answer
        .outdated
        .iter()
        .filter(|o| {
            known.contains(&o.id)
                && evidence.contains(&o.evidence_id)
                && !o.text.trim().is_empty()
                && seen.insert(o.id)
        })
        .map(|o| Outdated {
            id: o.id,
            evidence_id: o.evidence_id,
            text: o.text.trim().to_owned(),
        })
        .collect()
}

fn valid_tighten(
    answer: &JudgeAnswer,
    known: &HashSet<i64>,
    outdated: &[Outdated],
) -> Vec<Tighten> {
    let mut seen: HashSet<i64> = outdated.iter().map(|o| o.id).collect();
    answer
        .tighten
        .iter()
        .filter(|t| known.contains(&t.id) && !t.text.trim().is_empty() && seen.insert(t.id))
        .map(|t| Tighten {
            id: t.id,
            text: t.text.trim().to_owned(),
        })
        .collect()
}
