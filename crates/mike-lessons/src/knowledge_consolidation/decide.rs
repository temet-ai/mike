use super::rewrite::{Rewrite, plan_rewrites};
use super::reach::{not_reached_reason, reaches, scopes_clash};
use super::answer::{MergeGroup, PullKind, Relation, Validated};
use chrono::{DateTime, Duration, Utc};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stmt {
    pub id: i64,
    pub item_id: i64,
    pub kind: String,
    pub source_name: Option<String>,
    pub carrier: String,
    pub text: String,
    pub passage: Option<String>,
    pub passage_start: Option<i32>,
    pub passage_end: Option<i32>,
    pub stated_at: DateTime<Utc>,
    pub lesson_id: Option<i64>,
    pub lesson_active: bool,
    pub current: bool,
    pub project_id: Option<i64>,
    pub machine_wide: bool,
    pub relative_path: Option<String>,
    pub always_loaded: bool,
}

impl Stmt {
    pub fn is_lesson(&self) -> bool {
        self.kind == "lesson"
    }

    pub fn is_file(&self) -> bool {
        self.kind == "artifact"
    }

    pub fn is_active_lesson(&self) -> bool {
        self.is_lesson() && self.lesson_active
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceItem {
    pub id: i64,
    pub reference: String,
    pub kind: String,
    pub dated_at: DateTime<Utc>,
}

impl EvidenceItem {
    fn settles(&self, a: &Stmt, b: &Stmt) -> bool {
        matches!(
            self.kind.as_str(),
            "correction" | "unmet_ask" | "question_answer"
        ) && self.dated_at > a.stated_at
            && self.dated_at > b.stated_at
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Covered,
    Superseded,
}

impl Verdict {
    pub fn code(self) -> &'static str {
        match self {
            Self::Covered => "covered",
            Self::Superseded => "superseded",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    OperatorEvidence,
    Newer,
    FileOverLesson,
}

impl Rule {
    pub fn describe(self) -> &'static str {
        match self {
            Self::OperatorEvidence => "the operator's later words settle it",
            Self::Newer => "it was stated more than a day later",
            Self::FileOverLesson => "a hand-written file wins over a lesson",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Winner {
    pub winner: i64,
    pub loser: i64,
    pub rule: Rule,
}

pub fn decide_conflict(
    a: &Stmt,
    b: &Stmt,
    evidence_for: Option<i64>,
    evidence: &[EvidenceItem],
) -> Option<Winner> {
    let pair = |winner: &Stmt, rule| {
        let loser = if winner.id == a.id { b } else { a };
        Some(Winner {
            winner: winner.id,
            loser: loser.id,
            rule,
        })
    };
    if let Some(named) = [a, b].into_iter().find(|s| Some(s.id) == evidence_for)
        && evidence.iter().any(|e| e.settles(a, b))
    {
        return pair(named, Rule::OperatorEvidence);
    }
    let gap = a.stated_at - b.stated_at;
    if gap > Duration::days(1) {
        return pair(a, Rule::Newer);
    }
    if gap < -Duration::days(1) {
        return pair(b, Rule::Newer);
    }
    match (a.is_file(), b.is_file()) {
        (true, false) => pair(a, Rule::FileOverLesson),
        (false, true) => pair(b, Rule::FileOverLesson),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Retire {
        loser: i64,
        by: i64,
        verdict: Verdict,
        reason: String,
    },
    Relate {
        id: i64,
        counterpart: i64,
        reason: String,
    },
    OpenConflict {
        id: i64,
        counterpart: i64,
        reason: String,
    },
    Keep {
        id: i64,
    },
    Misgroup {
        id: i64,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
    pub actions: Vec<Action>,
    pub joins: Vec<i64>,
    pub merge_candidates: Vec<MergeGroup>,
    pub rewrites: Vec<Rewrite>,
}

struct Planner<'a> {
    stmts: &'a HashMap<i64, Stmt>,
    evidence: &'a [EvidenceItem],
    plan: Plan,
    pairs: HashSet<(i64, i64)>,
    retired: HashSet<i64>,
}

fn label(stmt: &Stmt) -> String {
    format!("{} ({})", stmt.carrier, stmt.kind)
}

impl Planner<'_> {
    fn retire(&mut self, loser: i64, by: i64, verdict: Verdict, reason: String) {
        if self.retired.insert(loser) {
            self.plan.actions.push(Action::Retire {
                loser,
                by,
                verdict,
                reason,
            });
        }
    }

    fn final_coverer(&self, id: i64, edges: &HashMap<i64, i64>) -> i64 {
        let mut path = vec![id];
        let mut at = id;
        while let Some(next) = edges.get(&at) {
            if let Some(pos) = path.iter().position(|p| p == next) {
                let cycle = &path[pos..];
                return cycle
                    .iter()
                    .filter_map(|c| self.stmts.get(c))
                    .min_by_key(|s| (s.stated_at, s.id))
                    .map_or(*next, |s| s.id);
            }
            path.push(*next);
            at = *next;
        }
        at
    }

    fn apply_covers(&mut self, covers: &[(i64, i64, String)]) {
        let edges: HashMap<i64, i64> = covers.iter().map(|(id, by, _)| (*id, *by)).collect();
        let lost: HashSet<i64> = self.retired.clone();
        for (id, _, note) in covers {
            let fin = self.final_coverer(*id, &edges);
            if fin == *id {
                self.plan.actions.push(Action::Keep { id: *id });
                continue;
            }
            let (Some(loser), Some(coverer)) = (self.stmts.get(id), self.stmts.get(&fin)) else {
                continue;
            };
            if !coverer.current || lost.contains(&fin) {
                self.plan.actions.push(Action::Relate {
                    id: *id,
                    counterpart: fin,
                    reason: format!("Kept: its coverer {} is retired, lost a conflict or is not current", coverer.carrier),
                });
                continue;
            }
            if reaches(loser, coverer) {
                let reason = join_note(&format!("Covered by {}", label(coverer)), note);
                self.retire(*id, fin, Verdict::Covered, reason);
            } else {
                self.plan.actions.push(Action::Relate {
                    id: *id,
                    counterpart: fin,
                    reason: not_reached_reason(loser, coverer),
                });
            }
        }
        self.drop_conflict_wins_over_retired();
    }

    fn drop_conflict_wins_over_retired(&mut self) {
        let covered_gone: HashSet<i64> = self
            .plan
            .actions
            .iter()
            .filter_map(|a| match a {
                Action::Retire { loser, verdict: Verdict::Covered, .. } => Some(*loser),
                _ => None,
            })
            .collect();
        let mut reopened = Vec::new();
        self.plan.actions.retain(|action| match action {
            Action::Retire { loser, by, verdict: Verdict::Superseded, .. }
                if covered_gone.contains(by) =>
            {
                reopened.push((*loser, *by));
                false
            }
            _ => true,
        });
        for (loser, by) in reopened {
            self.retired.remove(&loser);
            self.plan.actions.push(Action::OpenConflict {
                id: loser,
                counterpart: by,
                reason: "Contradicts a statement that is itself covered by another; judged again once that settles".to_owned(),
            });
        }
    }

    fn conflict(&mut self, id: i64, other: i64, evidence_for: Option<i64>, note: &str) {
        let (Some(a), Some(b)) = (self.stmts.get(&id), self.stmts.get(&other)) else {
            return;
        };
        if !self.pairs.insert((id.min(other), id.max(other))) {
            return;
        }
        let settled = if scopes_clash(a, b) {
            None
        } else {
            decide_conflict(a, b, evidence_for, self.evidence)
        };
        match settled {
            Some(won) => {
                let winner = if won.winner == id { a } else { b };
                let reason = join_note(
                    &format!(
                        "Contradicted by {}; {}",
                        label(winner),
                        won.rule.describe()
                    ),
                    note,
                );
                self.retire(won.loser, won.winner, Verdict::Superseded, reason);
            }
            None => self.plan.actions.push(Action::OpenConflict {
                id,
                counterpart: other,
                reason: join_note(
                    "Contradicts another statement and no precedence rule settles it",
                    note,
                ),
            }),
        }
    }

    fn relate(&mut self, id: i64, counterpart: i64, note: &str) {
        let Some(other) = self.stmts.get(&counterpart) else {
            return;
        };
        let reason = join_note(&format!("Adds a detail to {}", label(other)), note);
        self.plan.actions.push(Action::Relate {
            id,
            counterpart,
            reason,
        });
    }
}

fn join_note(head: &str, note: &str) -> String {
    let note = note.trim();
    if note.is_empty() {
        head.to_owned()
    } else {
        format!("{head}: {note}")
    }
}

pub fn plan(
    validated: &Validated,
    stmts: &HashMap<i64, Stmt>,
    evidence: &[EvidenceItem],
) -> Plan {
    let mut planner = Planner {
        stmts,
        evidence,
        plan: Plan::default(),
        pairs: HashSet::new(),
        retired: HashSet::new(),
    };
    let mut covers: Vec<(i64, i64, String)> = Vec::new();
    for member in &validated.members {
        match member.relation {
            Relation::Keep => planner.plan.actions.push(Action::Keep { id: member.id }),
            Relation::CoveredBy(by) => covers.push((member.id, by, member.note.clone())),
            Relation::AddsTo(by) => planner.relate(member.id, by, &member.note),
            Relation::ConflictsWith(by) => {
                planner.conflict(member.id, by, member.evidence_for, &member.note);
            }
            Relation::Elsewhere => planner.plan.actions.push(Action::Misgroup {
                id: member.id,
                reason: join_note("Does not belong in this subject", &member.note),
            }),
        }
    }
    for pull in &validated.pulled {
        planner.plan.joins.push(pull.id);
        match pull.kind {
            PullKind::CoveredByMember => covers.push((pull.id, pull.by, pull.note.clone())),
            PullKind::CoversMember => {}
            PullKind::Adds => planner.relate(pull.id, pull.by, &pull.note),
            PullKind::Conflicts => {
                planner.conflict(pull.id, pull.by, pull.evidence_for, &pull.note);
            }
        }
    }
    planner.apply_covers(&covers);
    let mut retired = planner.retired.clone();
    let mut plan = planner.plan;
    plan.rewrites = plan_rewrites(validated, stmts, evidence, &retired);
    retired.extend(plan.rewrites.iter().map(|r| r.id));
    plan.merge_candidates = acceptable_merges(validated, stmts, &retired);
    plan
}

pub fn acceptable_merges(
    validated: &Validated,
    stmts: &HashMap<i64, Stmt>,
    retired: &HashSet<i64>,
) -> Vec<MergeGroup> {
    let mut used: HashSet<i64> = HashSet::new();
    let mut out = Vec::new();
    for group in &validated.merges {
        let ids: HashSet<i64> = group.ids.iter().copied().collect();
        let members: Vec<&Stmt> = ids.iter().filter_map(|id| stmts.get(id)).collect();
        let text = group.text.trim();
        let allowed = ids.len() >= 2
            && members.len() == ids.len()
            && members.iter().all(|s| s.is_active_lesson())
            && ids.iter().all(|id| !retired.contains(id) && !used.contains(id))
            && !text.is_empty()
            && text.chars().count() <= members.iter().map(|s| s.text.chars().count()).sum();
        if allowed {
            used.extend(ids);
            out.push(MergeGroup {
                ids: group.ids.clone(),
                text: text.to_owned(),
            });
        }
    }
    out
}

pub fn oldest<'a>(stmts: &'a HashMap<i64, Stmt>, ids: &[i64]) -> Option<&'a Stmt> {
    ids.iter()
        .filter_map(|id| stmts.get(id))
        .min_by_key(|s| (s.stated_at, s.id))
}

pub fn merge_check_prompt(group_texts: &[&str], merged: &str) -> String {
    let statements: Vec<String> = group_texts
        .iter()
        .enumerate()
        .map(|(i, t)| format!("[{}] {t}", i + 1))
        .collect();
    format!(
        "STATEMENTS\n{}\n\nTEXT\n{merged}\n\nList each instruction in the STATEMENTS that TEXT does not give. Empty when TEXT gives them all.",
        statements.join("\n")
    )
}
