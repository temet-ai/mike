use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::patching::patch;
use super::trigger::TriggerEval;
use super::{EvalError, Material};
use crate::llm::OpenRouterClient;
use crate::store::persistence::lessons::skill_draft_eval::{AgentScope, ReplacedDraft};
use crate::store::persistence::lessons::skill_drafts::DraftLesson;

pub use crate::settings::memory_prompts::SKILL_REVIEW_PROMPT as PROMPT;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Approve,
    Patch,
    NothingToSave,
    WrongPlace,
    Unusable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Place {
    Rule,
    ExistingSkill,
    AgentPersona,
    MemoryFile,
    Nowhere,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Review {
    pub decision: Decision,
    pub reason: String,
    #[serde(default)]
    pub overlap: Option<String>,
    #[serde(default)]
    pub belongs_in: Option<Place>,
    pub patched_description: Option<String>,
    pub patched_skill_md: Option<String>,
    pub original_description: Option<String>,
    pub original_skill_md: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Reply {
    pub decision: Reviewed,
    pub reason: String,
    pub overlap: String,
    pub belongs_in: Belongs,
    pub skill_md: String,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reviewed {
    Approve,
    Patch,
    NothingToSave,
    WrongPlace,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Belongs {
    None,
    Rule,
    ExistingSkill,
    AgentPersona,
    MemoryFile,
    Nowhere,
}

impl Belongs {
    fn place(self) -> Option<Place> {
        match self {
            Self::None => None,
            Self::Rule => Some(Place::Rule),
            Self::ExistingSkill => Some(Place::ExistingSkill),
            Self::AgentPersona => Some(Place::AgentPersona),
            Self::MemoryFile => Some(Place::MemoryFile),
            Self::Nowhere => Some(Place::Nowhere),
        }
    }
}

pub struct Context<'a> {
    pub skills: &'a [(String, String)],
    pub replaced: Option<&'a ReplacedDraft>,
    pub agent: Option<&'a AgentScope>,
    pub project: &'a str,
}

fn schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "decision": {"type": "string", "enum": ["approve", "patch", "nothing_to_save", "wrong_place"]},
            "reason": {"type": "string"},
            "overlap": {"type": "string"},
            "belongs_in": {"type": "string", "enum": ["none", "rule", "existing_skill", "agent_persona", "memory_file", "nowhere"]},
            "skill_md": {"type": "string"}
        },
        "required": ["decision", "reason", "overlap", "belongs_in", "skill_md"]
    })
}

fn block(out: &mut String, label: &str, body: &str) {
    let _ = write!(
        out,
        "<<{label}>>\n{}\n<</{label}>>\n\n",
        body.trim().replace("<<", "< <")
    );
}

fn skills_text(skills: &[(String, String)]) -> String {
    if skills.is_empty() {
        return "No other skill is known for this project.".to_owned();
    }
    let mut out = String::new();
    for (name, description) in skills {
        let _ = writeln!(out, "- {name}: {}", description.trim());
    }
    out
}

fn replaced_text(replaced: &ReplacedDraft) -> String {
    format!(
        "This draft would replace the reviewed skill {} that the project has now.\nIts description: {}\n\n{}\n",
        replaced.folder,
        replaced.description.trim(),
        replaced.skill_md.trim()
    )
}

fn agent_text(agent: &AgentScope) -> String {
    let mut out = format!("The lessons are scoped to the agent {}.\n", agent.name);
    if let Some(description) = &agent.description {
        let _ = writeln!(out, "Its description: {}", description.trim());
    }
    if let Some(path) = &agent.definition_path {
        let _ = writeln!(out, "Its definition file: {path}");
    }
    out
}

pub fn input(
    material: &Material,
    lessons: &[DraftLesson],
    first: &TriggerEval,
    context: &Context<'_>,
) -> String {
    let mut out = String::new();
    block(&mut out, "draft-skill-md", &material.skill_md);
    for reference in &material.references {
        let label = format!("reference-file references/{}.md", reference.name);
        block(&mut out, &label, &reference.content);
    }
    let mut listed = String::new();
    for lesson in lessons {
        let _ = writeln!(
            listed,
            "[{}] ({}) {}\n",
            lesson.id,
            lesson.kind,
            lesson.text.trim()
        );
    }
    block(&mut out, "lessons", &listed);
    block(&mut out, "existing-skills", &skills_text(context.skills));
    if let Some(replaced) = context.replaced {
        block(&mut out, "replaced-skill", &replaced_text(replaced));
    }
    if !context.project.trim().is_empty() {
        block(&mut out, "project", context.project);
    }
    if let Some(agent) = context.agent {
        block(&mut out, "agent-scope", &agent_text(agent));
    }
    block(&mut out, "trigger-test", &trigger_result_text(first));
    out
}

fn trigger_result_text(first: &TriggerEval) -> String {
    let findings = super::verdict::findings_of(&first.results);
    let mut out = format!(
        "{} of {} test messages were answered as labelled (score {:.2}).\n",
        first.correct, first.total, first.score
    );
    let described = findings.describe();
    if described.trim().is_empty() {
        out.push_str("No message was missed or wrongly matched.\n");
    } else {
        out.push_str(&described);
    }
    out
}

fn blank(decision: Decision, reason: String) -> Review {
    Review {
        decision,
        reason,
        overlap: None,
        belongs_in: None,
        patched_description: None,
        patched_skill_md: None,
        original_description: None,
        original_skill_md: None,
    }
}

fn named(text: &str) -> Option<String> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

pub fn settled(material: &Material, reply: Reply) -> Review {
    let reason = reply.reason.trim().to_owned();
    let overlap = named(&reply.overlap);
    let belongs_in = reply.belongs_in.place();
    match reply.decision {
        Reviewed::Approve => blank(Decision::Approve, reason),
        Reviewed::NothingToSave => Review {
            overlap,
            ..blank(Decision::NothingToSave, reason)
        },
        Reviewed::WrongPlace => match belongs_in {
            Some(_) => Review {
                overlap,
                belongs_in,
                ..blank(Decision::WrongPlace, reason)
            },
            None => blank(
                Decision::Unusable,
                "the reviewer answered wrong_place without saying where the content belongs"
                    .to_owned(),
            ),
        },
        Reviewed::Patch => match patch(material, &reply.skill_md) {
            Ok(patched) => Review {
                patched_description: Some(patched.description),
                patched_skill_md: Some(patched.skill_md),
                original_description: Some(material.description.clone()),
                original_skill_md: Some(material.skill_md.clone()),
                ..blank(Decision::Patch, reason)
            },
            Err(problem) => blank(
                Decision::Unusable,
                format!("the reviewer's patch cannot ship: {problem}"),
            ),
        },
    }
}

pub async fn review(
    llm: &OpenRouterClient,
    prompt: &str,
    material: &Material,
    lessons: &[DraftLesson],
    first: &TriggerEval,
    context: &Context<'_>,
) -> Result<Review, EvalError> {
    let reply: Reply = llm
        .verdict(prompt, &input(material, lessons, first, context), schema())
        .await?;
    Ok(settled(material, reply))
}
