use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use chrono::NaiveDate;

use super::budget::{Candidates, hold_overlapping};
use super::checkout::{Eligible, local_checkout};
use super::drafts::{
    DraftSets, TopicCandidate, chosen_draft, drafted_references, feeds_topic_skill, keep_reviewed,
};
use super::ledger::{Ledger, LedgerEntry, hold};
use super::model::{DraftRow, LessonRow, Recipe, skill_key};
use super::render::{
    lessons_reference, recipe_skill, shared_memory, shared_skill, skill_folder, slug, topic_skill,
};

pub(super) struct PlanInputs<'a> {
    pub today: NaiveDate,
    pub lessons: &'a [LessonRow],
    pub drafts: &'a DraftSets,
    pub policies: &'a BTreeMap<String, String>,
    pub user_retired: &'a BTreeSet<String>,
    pub locations: &'a BTreeMap<String, Vec<String>>,
    pub recipes: &'a [Recipe],
    pub skill_roots: &'a [PathBuf],
    pub runner: &'a Path,
    pub eligible: Eligible<'a>,
}

pub(super) struct Emission {
    pub checkout: PathBuf,
    pub folder: String,
    pub path: PathBuf,
}

#[derive(Default)]
pub(super) struct SkillPlan {
    pub desired: BTreeMap<String, String>,
    pub emitted: BTreeMap<String, Emission>,
    pub extra: BTreeMap<String, Vec<String>>,
    pub authored: BTreeSet<String>,
    pub fresh: BTreeSet<String>,
    pub candidate_lessons: BTreeMap<String, usize>,
    pub held_by_user: Vec<String>,
    pub held_for_approval: Vec<String>,
    pub held_retired: Vec<String>,
    pub skipped: BTreeSet<String>,
    pub purpose_held: BTreeMap<String, String>,
    pub excludes: BTreeMap<PathBuf, Vec<String>>,
}

enum Verdict<'a> {
    HeldByUser,
    HeldRetired,
    HeldForApproval,
    Emit(Option<&'a DraftRow>),
}

pub(super) fn group_by_project(lessons: &[LessonRow]) -> BTreeMap<String, Vec<LessonRow>> {
    let mut projects: BTreeMap<String, Vec<LessonRow>> = BTreeMap::new();
    for row in lessons {
        projects
            .entry(row.project_uuid.to_string())
            .or_default()
            .push(row.clone());
    }
    projects
}

fn group_topics(lessons: &[LessonRow]) -> BTreeMap<String, Vec<LessonRow>> {
    let mut topics: BTreeMap<String, Vec<LessonRow>> = BTreeMap::new();
    for row in lessons {
        let Some(topic) = row.topic.as_deref().filter(|t| !t.trim().is_empty()) else {
            continue;
        };
        if topic.starts_with("shared:") || topic.starts_with("verifier:") {
            continue;
        }
        let key = slug(topic);
        if feeds_topic_skill(row, &key) {
            topics.entry(key).or_default().push(row.clone());
        }
    }
    topics
}

fn topic_candidates(
    inputs: &PlanInputs<'_>,
    projects: &BTreeMap<String, Vec<LessonRow>>,
) -> BTreeMap<String, TopicCandidate> {
    let mut candidates = BTreeMap::new();
    for (project_uuid, lessons) in projects {
        let Some(first) = lessons.first() else {
            continue;
        };
        let template = TopicCandidate {
            project: first.project_path.clone(),
            folder: String::new(),
            rows: Vec::new(),
            roots: inputs
                .locations
                .get(project_uuid)
                .cloned()
                .unwrap_or_default(),
        };
        for (topic_slug, rows) in group_topics(lessons) {
            if rows.len() >= 2 {
                let folder = skill_folder(&topic_slug, &rows);
                candidates.insert(
                    skill_key(&first.project_uuid, &folder),
                    TopicCandidate {
                        folder,
                        rows,
                        ..template.clone()
                    },
                );
            }
        }
        keep_reviewed(
            project_uuid,
            lessons,
            inputs.drafts,
            &mut candidates,
            &template,
        );
    }
    candidates
}

fn plan_shared(inputs: &PlanInputs<'_>, plan: &mut SkillPlan) {
    let shared: Vec<LessonRow> = inputs
        .lessons
        .iter()
        .filter(|row| {
            row.agent_id.is_none()
                && row
                    .topic
                    .as_deref()
                    .is_some_and(|topic| topic.starts_with("shared:"))
        })
        .cloned()
        .collect();
    if shared.is_empty() {
        return;
    }
    let (memory, skill) = (shared_memory(&shared), shared_skill(&shared));
    for root in inputs.skill_roots {
        let base = root.join("shared-workflows");
        plan.desired.insert(
            base.join("references/memory.md").display().to_string(),
            memory.clone(),
        );
        plan.desired
            .insert(base.join("SKILL.md").display().to_string(), skill.clone());
    }
}

fn plan_recipes(inputs: &PlanInputs<'_>, plan: &mut SkillPlan) {
    for recipe in inputs.recipes {
        let (name, body) = recipe_skill(recipe, inputs.runner);
        let checkout = PathBuf::from(&recipe.cwd);
        if !(inputs.eligible)(&checkout) {
            continue;
        }
        for skills in [".claude/skills", ".codex/skills"] {
            let file = checkout.join(skills).join(&name).join("SKILL.md");
            plan.desired
                .insert(file.display().to_string(), body.clone());
            plan.excludes
                .entry(checkout.clone())
                .or_default()
                .push(format!("{skills}/{name}/"));
        }
    }
}

fn verdict<'a>(inputs: &PlanInputs<'a>, ledger: &Ledger, key: &str, watermark: i64) -> Verdict<'a> {
    if inputs.user_retired.contains(key) {
        return Verdict::HeldByUser;
    }
    let project = key.split(':').next().unwrap_or_default();
    if let Some(entry) = ledger.get(key)
        && entry.retired_at.is_some()
        && matches!(
            entry.retired_reason.as_deref(),
            None | Some("unloaded") | Some("corrected_above_base")
        )
        && watermark <= entry.lesson_watermark
    {
        return Verdict::HeldRetired;
    }
    let auto = inputs.policies.get(project).map_or("ask", String::as_str) == "auto";
    let drafts = inputs.drafts;
    let draft = chosen_draft(
        auto,
        drafts.accepted.get(key),
        drafts.approved.get(key),
        drafts.rejected.get(key),
    );
    match (draft, auto) {
        (None, true) if drafts.rejected.contains_key(key) => Verdict::HeldByUser,
        (None, false) => Verdict::HeldForApproval,
        _ => Verdict::Emit(draft),
    }
}

fn emit(
    inputs: &PlanInputs<'_>,
    ledger: &mut Ledger,
    plan: &mut SkillPlan,
    key: &str,
    candidate: &TopicCandidate,
) {
    let Some(checkout) = local_checkout(&candidate.project, &candidate.roots, inputs.eligible)
    else {
        plan.skipped.insert(candidate.project.clone());
        return;
    };
    let watermark = candidate.rows.iter().map(|row| row.id).max().unwrap_or(0);
    match verdict(inputs, ledger, key, watermark) {
        Verdict::HeldByUser => {
            hold(ledger, key, "user", inputs.today);
            plan.held_by_user.push(key.to_owned());
        }
        Verdict::HeldRetired => plan.held_retired.push(key.to_owned()),
        Verdict::HeldForApproval => {
            hold(ledger, key, "policy", inputs.today);
            plan.held_for_approval.push(key.to_owned());
        }
        Verdict::Emit(draft) => {
            write_skill(
                inputs,
                ledger,
                plan,
                (key, candidate, &checkout),
                (watermark, draft),
            );
        }
    }
}

fn write_skill(
    inputs: &PlanInputs<'_>,
    ledger: &mut Ledger,
    plan: &mut SkillPlan,
    target: (&str, &TopicCandidate, &Path),
    version: (i64, Option<&DraftRow>),
) {
    let (key, candidate, checkout) = target;
    let (watermark, draft) = version;
    let renew = ledger
        .get(key)
        .is_none_or(|entry| entry.retired_at.is_some());
    if renew {
        ledger.insert(
            key.to_owned(),
            LedgerEntry {
                first_emitted: inputs.today.to_string(),
                ..LedgerEntry::default()
            },
        );
        plan.fresh.insert(key.to_owned());
    }
    let folder = &candidate.folder;
    let skill_dir = checkout.join(".claude/skills").join(folder);
    let path = skill_dir.join("SKILL.md");
    if let Some(entry) = ledger.get_mut(key) {
        entry.lesson_watermark = watermark;
        entry.path = Some(path.display().to_string());
        entry.draft_id = draft.map(|found| found.id);
        entry.retired_reason = None;
    }
    let checkout_name = checkout.display().to_string();
    let topic = candidate
        .rows
        .first()
        .and_then(|row| row.topic.clone())
        .unwrap_or_else(|| folder.clone());
    let skill_md = match draft {
        Some(found) => found.skill_md.clone(),
        None => topic_skill(folder, &checkout_name, &topic, &candidate.rows),
    };
    let reference = skill_dir.join("references/lessons.md");
    let mut extra = vec![reference.display().to_string()];
    plan.desired.insert(path.display().to_string(), skill_md);
    plan.desired.insert(
        reference.display().to_string(),
        lessons_reference(&checkout_name, folder, &candidate.rows),
    );
    if let Some(found) = draft {
        plan.authored.insert(key.to_owned());
        for (name, content) in drafted_references(found) {
            let file = skill_dir.join("references").join(format!("{name}.md"));
            plan.desired.insert(file.display().to_string(), content);
            extra.push(file.display().to_string());
        }
    }
    plan.extra.insert(key.to_owned(), extra);
    plan.candidate_lessons
        .insert(key.to_owned(), candidate.rows.len());
    plan.emitted.insert(
        key.to_owned(),
        Emission {
            checkout: checkout.to_path_buf(),
            folder: folder.clone(),
            path,
        },
    );
}

fn overlap_candidates(candidates: &BTreeMap<String, TopicCandidate>) -> Candidates {
    candidates
        .iter()
        .map(|(key, candidate)| {
            let topic = candidate
                .rows
                .first()
                .and_then(|row| row.topic.clone())
                .unwrap_or_else(|| candidate.folder.clone());
            (key.clone(), (topic, candidate.rows.len()))
        })
        .collect()
}

pub(super) fn plan_skills(inputs: &PlanInputs<'_>, ledger: &mut Ledger) -> SkillPlan {
    let mut plan = SkillPlan::default();
    let projects = group_by_project(inputs.lessons);
    let candidates = topic_candidates(inputs, &projects);
    plan_shared(inputs, &mut plan);
    plan_recipes(inputs, &mut plan);
    let drafts = inputs.drafts;
    let shipped: BTreeSet<String> = candidates
        .keys()
        .filter(|key| {
            drafts.accepted.contains_key(*key)
                || drafts.approved.contains_key(*key)
                || ledger
                    .get(*key)
                    .is_some_and(|entry| entry.retired_at.is_none())
        })
        .cloned()
        .collect();
    plan.purpose_held = hold_overlapping(&overlap_candidates(&candidates), &shipped);
    for (key, candidate) in &candidates {
        if !plan.purpose_held.contains_key(key) {
            emit(inputs, ledger, &mut plan, key, candidate);
        }
    }
    plan
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod plan_tests;

#[cfg(test)]
#[path = "plan_policy_tests.rs"]
mod plan_policy_tests;
