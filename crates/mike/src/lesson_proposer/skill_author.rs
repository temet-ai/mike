use crate::store::persistence::lessons_repo::LessonsPersist as _;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::llm::OpenRouterClient;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::lessons::skill_drafts::{
    DraftLesson, DraftTarget, NewSkillDraft, PreviousDraft,
};
use crate::store::persistence::lessons::skill_live::RetiredSkill;
use crate::store::persistence::lessons::skill_verdicts::VerdictFollowThrough;

#[path = "skill_author_previous.rs"]
mod previous;
#[path = "skill_author_retired.rs"]
mod retired;
#[path = "skill_shape.rs"]
mod shape;
#[cfg(test)]
#[path = "skill_author_tests.rs"]
mod tests;

pub use shape::{
    MAX_DESCRIPTION_CHARS, MAX_OVERVIEW_CHARS, MAX_REFERENCE_CHARS, MAX_REFERENCES, acceptable,
};

pub const MARKER: &str = "<!-- mike-learning-generated:v1 -->";
const PROMPT_EXCERPT_CHARS: usize = 400;

pub const AUTHOR_PROMPT: &str = "You write a Claude Code skill from a project's accepted lessons, \
following the skill-creator method.\n\n\
A skill loads in three levels, and you write for each. The name and description are always in \
context and decide whether the skill loads at all. The body of SKILL.md is read every time the \
skill loads. Files under references/ are read only when SKILL.md sends the reader to one. So \
SKILL.md holds only what every use of the skill needs and points to the rest; the detailed \
instructions live in reference files that the reader opens when their situation applies.\n\n\
description: say what the skill does and then name the concrete situations in which to use it, \
drawn from the lessons and the prompts that produced them. Be a little pushy: Claude tends to \
under-use skills, so name the situations even when the user would not say the topic's name. \
Cover reviewing, auditing and checking work in this area and answering questions about it, as \
well as doing the work: a session asked to review the migrations added this week or to say which \
toggles start enabled needs this skill as much as one adding a migration. Everyday words; no \
counts, no field names, no project id; keep the description under 1,024 characters.\n\n\
overview: the body of SKILL.md, under 60 lines. In a few sentences say what the skill covers \
and why it matters, then give the steps every task in this area takes, if there are any. End \
with a list of the reference files, one line each, written as a link followed by when to read \
it: `- [references/<name>.md](references/<name>.md): read when ...`. Every reference file must \
be in that list. Do not repeat a reference file's instructions in the overview. For the \
evidence behind the rules, point to references/lessons.md, which the caller writes with every \
lesson and its source turn.\n\n\
references: two to six files that split the lessons by the situation they apply to (a kind of \
task, a tool, a failure to avoid), not by lesson kind. Each has a name (lowercase words joined \
by hyphens, never `lessons`), when_to_read (the situation, as the end of the sentence `Read \
this when ...`, for example `you are adding or renumbering a migration.`) and content: the \
instructions for that situation in the imperative, written for a capable reader. Explain why \
each instruction matters, in the lessons' own terms, so the reader can handle a case the \
lessons did not foresee; the reason works better than capital-letter MUSTs. Keep every \
concrete fact a lesson carries (a command, a path, a measured number, a date, a commit), \
because those are what make a rule checkable, and write a repeatable check as a step with its \
exact command. End each file with the lesson ids it draws on, as `Lessons: 12, 40.` Content is \
Markdown without a title line; the caller adds the title.\n\n\
Then write what the evaluation needs. trigger_queries: twenty realistic user messages, eight to ten that \
should load this skill without naming the topic, and eight to ten near misses that share words \
with it but need something else. Of the messages that should load it, write at least a third as \
reviews, audits, checks or questions about the area, not requests to do the work, for example \
`review the migrations added this week` or `which toggles start enabled`.\n\n\
When the input has a PREVIOUS DRAFT (REJECTED) section, an earlier draft for this topic was \
evaluated and failed. Its evaluation lists the user messages that should have loaded the skill \
and did not, the messages that loaded it and should not have, and the reviewer's decision and \
reason, sometimes with the reviewer's corrected SKILL.md. Write a new draft that fixes those \
failures: change the description so the missed messages match and the wrongly matched ones do \
not, and change the body and reference files so the reviewer's objection no longer holds. Keep \
what worked; do not drift from the lessons to satisfy a message.\n\n\
When the input has a RETIRED SKILL section, a skill for this topic shipped and was retired \
because of how it did in real sessions: either nothing loaded it, or the sessions that loaded \
it were corrected more often than the project's turns in general. Its numbers, description and \
SKILL.md follow. Work out from them why it did not help (a description that never matched the \
work people did, instructions that were generic, wrong or in the way) and write a different \
skill that avoids that fault, not a rewording of the same one.\n\n\
Return JSON matching the schema. The overview is Markdown without a title line and without \
frontmatter; the caller adds both.";

#[derive(Debug, Deserialize, Serialize)]
pub struct TriggerQuery {
    pub query: String,
    pub should_trigger: bool,
}

#[derive(Debug, Deserialize)]
pub struct AuthoredReference {
    pub name: String,
    pub when_to_read: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct AuthoredSkill {
    pub description: String,
    pub overview: String,
    pub references: Vec<AuthoredReference>,
    pub trigger_queries: Vec<TriggerQuery>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct AuthorPassReport {
    pub verdicts: VerdictFollowThrough,
    pub folder: Option<String>,
    pub project_id: Option<i64>,
    pub lessons: usize,
    pub draft_id: Option<i64>,
    pub error: Option<String>,
}

fn string_fields(names: &[&str]) -> Value {
    let properties: serde_json::Map<String, Value> = names
        .iter()
        .map(|n| ((*n).to_owned(), json!({"type": "string"})))
        .collect();
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": properties,
        "required": names
    })
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "description": {"type": "string"},
            "overview": {"type": "string"},
            "references": {"type": "array", "items": string_fields(&["name", "when_to_read", "content"])},
            "trigger_queries": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "query": {"type": "string"},
                        "should_trigger": {"type": "boolean"}
                    },
                    "required": ["query", "should_trigger"]
                }
            }
        },
        "required": ["description", "overview", "references", "trigger_queries"]
    })
}

fn excerpt(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_owned();
    }
    let cut: String = trimmed.chars().take(max_chars).collect();
    format!("{cut} [cut]")
}

pub fn build_input(
    target: &DraftTarget,
    lessons: &[DraftLesson],
    previous: Option<&PreviousDraft>,
    retired: Option<&RetiredSkill>,
) -> String {
    let project = std::path::Path::new(&target.project_path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| target.project_path.clone());
    let mut out = String::new();
    let _ = writeln!(out, "## PROJECT\n{project}\n");
    let _ = writeln!(out, "## TOPIC\n{}\n", target.topic);
    if let Some(agent) = &target.agent_name {
        let _ = writeln!(
            out,
            "## AGENT\nThis skill applies only to the {agent} agent.\n"
        );
    }
    out.push_str("## LESSONS\n");
    for lesson in lessons {
        let _ = writeln!(
            out,
            "[{}] ({}) {}\n",
            lesson.id,
            lesson.kind,
            lesson.text.trim()
        );
    }
    let prompts: Vec<&str> = lessons
        .iter()
        .filter_map(|l| l.user_prompt.as_deref())
        .collect();
    if !prompts.is_empty() {
        out.push_str("## PROMPTS THAT PRODUCED THESE LESSONS\n");
        for prompt in prompts {
            let _ = writeln!(out, "- {}", excerpt(prompt, PROMPT_EXCERPT_CHARS));
        }
    }
    if let Some(previous) = previous {
        out.push('\n');
        out.push_str(&previous::section(previous));
    }
    if let Some(retired) = retired {
        out.push('\n');
        out.push_str(&retired::section(retired));
    }
    out
}

fn title(name: &str) -> String {
    let words = name.replace('-', " ");
    let mut chars = words.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + chars.as_str()
    })
}

pub fn skill_md(folder: &str, description: &str, overview: &str) -> String {
    format!(
        "---\nname: {folder}\ndescription: {}\n---\n{MARKER}\n\n# {}\n\n{}\n",
        serde_json::to_string(description.trim()).unwrap_or_default(),
        title(folder),
        overview.trim()
    )
}

pub fn reference_md(reference: &AuthoredReference) -> String {
    format!(
        "# {}\n\nRead this when {}\n\n{}\n",
        title(&reference.name),
        reference.when_to_read.trim(),
        reference.content.trim()
    )
}

pub fn reference_files(authored: &AuthoredSkill) -> Value {
    Value::Array(
        authored
            .references
            .iter()
            .map(|r| {
                json!({
                    "name": r.name,
                    "when_to_read": r.when_to_read.trim(),
                    "content": reference_md(r)
                })
            })
            .collect(),
    )
}

pub async fn run_pass(persist: &PersistHandle, llm: &OpenRouterClient) -> AuthorPassReport {
    let mut report = AuthorPassReport {
        verdicts: follow_place_verdicts(persist).await,
        ..AuthorPassReport::default()
    };
    let target = match persist.lessons().next_skill_draft_target().await {
        Ok(Some(target)) => target,
        Ok(None) => return report,
        Err(e) => return failed(report, format!("target: {e}")),
    };
    report.folder = Some(target.folder.clone());
    report.project_id = Some(target.project_id);
    match author(persist, llm, &target, &mut report).await {
        Ok(id) => written(report, id),
        Err(error) => {
            record_failure(persist, &target, &error).await;
            failed(report, error)
        }
    }
}

async fn follow_place_verdicts(persist: &PersistHandle) -> VerdictFollowThrough {
    match persist.lessons().apply_skill_place_verdicts().await {
        Ok(done) => {
            log_verdicts_applied(&done);
            done
        }
        Err(error) => {
            log_verdicts_not_applied(&error);
            VerdictFollowThrough::default()
        }
    }
}

fn log_verdicts_applied(done: &VerdictFollowThrough) {
    if *done == VerdictFollowThrough::default() {
        return;
    }
    tracing::info!(
        placed_as_rule = done.placed_as_rule,
        moved_to_skill = done.moved_to_skill,
        "skill author: reviewer placement verdicts applied to their lessons"
    );
}

fn log_verdicts_not_applied(error: &impl std::fmt::Display) {
    tracing::warn!(%error, "skill author: reviewer placement verdicts not applied");
}

fn written(mut report: AuthorPassReport, draft_id: i64) -> AuthorPassReport {
    report.draft_id = Some(draft_id);
    tracing::info!(
        draft_id,
        folder = ?report.folder,
        lessons = report.lessons,
        "skill author: draft written"
    );
    report
}

async fn record_failure(persist: &PersistHandle, target: &DraftTarget, error: &str) {
    if let Err(e) = persist
        .lessons()
        .record_skill_draft_failure(target, error)
        .await
    {
        tracing::warn!(error = %e, folder = target.folder, "skill author: failure not recorded");
    }
}

async fn author(
    persist: &PersistHandle,
    llm: &OpenRouterClient,
    target: &DraftTarget,
    report: &mut AuthorPassReport,
) -> Result<i64, String> {
    let lessons = persist
        .lessons()
        .skill_draft_lessons(target)
        .await
        .map_err(|e| format!("lessons: {e}"))?;
    if lessons.len() < 2 {
        return Err(format!("{} lessons, need 2", lessons.len()));
    }
    report.lessons = lessons.len();
    let previous = persist
        .lessons()
        .previous_rejected_skill_draft(target)
        .await
        .map_err(|e| format!("previous draft: {e}"))?;
    let retired = persist
        .lessons()
        .retired_skill_evidence(target)
        .await
        .map_err(|e| format!("retired skill: {e}"))?;
    let (mut authored, usage) = llm
        .verdict_with_usage::<AuthoredSkill>(
            AUTHOR_PROMPT,
            &build_input(target, &lessons, previous.as_ref(), retired.as_ref()),
            schema(),
        )
        .await
        .map_err(|e| format!("model: {e}"))?;
    shape::normalise_reference_names(&mut authored);
    acceptable(&authored).map_err(|reason| format!("rejected draft: {reason}"))?;
    let lesson_ids: Vec<i64> = lessons.iter().map(|l| l.id).collect();
    let skill_md = skill_md(&target.folder, &authored.description, &authored.overview);
    let references = reference_files(&authored);
    let trigger_queries = serde_json::to_value(&authored.trigger_queries).unwrap_or(Value::Null);
    persist
        .lessons()
        .insert_skill_draft(NewSkillDraft {
            target,
            lesson_ids: &lesson_ids,
            model: llm.model(),
            description: authored.description.trim(),
            skill_md: &skill_md,
            reference_files: &references,
            trigger_queries: &trigger_queries,
            generation_id: usage.generation_id.as_deref(),
            cost_usd: usage.cost_usd,
        })
        .await
        .map_err(|e| format!("write: {e}"))
}

fn failed(mut report: AuthorPassReport, error: String) -> AuthorPassReport {
    tracing::warn!(error = %error, folder = ?report.folder, "skill author: pass failed");
    report.error = Some(error);
    report
}
