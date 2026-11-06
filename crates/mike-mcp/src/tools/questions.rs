use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{object_schema, parse_args_required, wrap_success};
use crate::client::InspectClient;
use crate::error::McpError;

pub const TOOL_ASK: &str = "ask";
pub const TOOL_ANSWER: &str = "answer";
pub const TOOL_DECLINE: &str = "decline";

pub const QUESTION_TOOL_NAMES: &[&str] = &[TOOL_ASK, TOOL_ANSWER, TOOL_DECLINE];

pub fn owns(name: &str) -> bool {
    QUESTION_TOOL_NAMES.contains(&name)
}

const FROM: &str = "Your own session id (full uuid or prefix of 8 or more characters). Omit to use the trusted MCP caller session; required when caller context is unavailable.";

pub fn question_tools() -> Vec<Tool> {
    vec![ask_tool(), answer_tool(), decline_tool()]
}

fn ask_tool() -> Tool {
    Tool::new(
        Cow::Borrowed(TOOL_ASK),
        Cow::Borrowed(
            "Ask another seat a question and carry on. Writes the question as a record with a \
             state (open, answered, declined) and mails the recipient once with the question id. \
             The recipient sees it under its open questions in `what_changed` view `standing` \
             until it answers or declines; you are mailed once when it does. `to`: a session id \
             or prefix, `task:<id>` (the seat of that task), `owner:<path>` (the seat whose task \
             owns the file, else the coordinator) or `reference` (the fleet's reference seat, \
             which answers where and how questions). `task:`, `owner:` and `reference` resolve \
             inside your own running fleet. `blocked: true` when you cannot go on without the \
             answer: a blocked question open for five minutes is passed once to the reference \
             seat and listed in the fleet digest. `about`: the id of a note the question is \
             about (a brief part, a finding); the recipient sees the note's title with the \
             question. Returns `question_id`, `state`, `to_session`, `to_task`, `run_id` and \
             whether the recipient was `mailed`. Refused: an empty question, an unknown session \
             or note, a `to` that names no seat yet, and `fleet` (a question goes to one seat).",
        ),
        object_schema(
            vec![
                ("from", json!({"type": "string", "description": FROM})),
                (
                    "to",
                    json!({"type": "string", "description": "A session id or prefix, task:<id>, owner:<path> or reference."}),
                ),
                (
                    "question",
                    json!({"type": "string", "description": "The question, whole."}),
                ),
                (
                    "blocked",
                    json!({"type": "boolean", "description": "True when you cannot go on without the answer. Default false."}),
                ),
                (
                    "about",
                    json!({"type": "string", "description": "Note id the question is about."}),
                ),
            ],
            &["to", "question"],
        ),
    )
}

fn answer_tool() -> Tool {
    Tool::new(
        Cow::Borrowed(TOOL_ANSWER),
        Cow::Borrowed(
            "Answer an open question and mail the asker once. Anyone in the project may answer, \
             not only the recipient (the reference seat answers escalated questions); the answer \
             records who did. `amend: true`, for a question about a brief note whose answer \
             changes the brief, also adds the question and the answer to the end of that brief \
             part as a new version of the note, so the next seat that reads the brief sees it; \
             the answer returns `amended_note` and `amended_version`, or `amend_error` when the \
             brief could not be changed (the answer itself still stands). Returns `question_id`, \
             `state` and whether the asker was `mailed_asker`. Refused: an empty answer, an \
             unknown question or session, a question already settled (nothing changes; the \
             answer names its state), and `amend` on a question that is not about a brief part.",
        ),
        object_schema(
            vec![
                ("from", json!({"type": "string", "description": FROM})),
                (
                    "question_id",
                    json!({"type": "string", "description": "The question id from the mail or from `what_changed` view `standing`."}),
                ),
                (
                    "answer",
                    json!({"type": "string", "description": "The answer, whole."}),
                ),
                (
                    "amend",
                    json!({"type": "boolean", "description": "Add the answer to the brief part the question is about. Default false."}),
                ),
            ],
            &["question_id", "answer"],
        ),
    )
}

fn decline_tool() -> Tool {
    Tool::new(
        Cow::Borrowed(TOOL_DECLINE),
        Cow::Borrowed(
            "Decline an open question you will not answer, and mail the asker once, so the \
             asker stops waiting and asks someone else. `reason` is optional and goes to the \
             asker. Returns `question_id`, `state` and whether the asker was `mailed_asker`. \
             Refused: an unknown question or session, and a question already settled (nothing \
             changes; the answer names its state).",
        ),
        object_schema(
            vec![
                ("from", json!({"type": "string", "description": FROM})),
                (
                    "question_id",
                    json!({"type": "string", "description": "The question id."}),
                ),
                (
                    "reason",
                    json!({"type": "string", "description": "Why it is declined."}),
                ),
            ],
            &["question_id"],
        ),
    )
}

#[derive(Debug, Deserialize)]
struct AskArgs {
    from: String,
    to: String,
    question: String,
    #[serde(default)]
    blocked: bool,
    #[serde(default)]
    about: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnswerArgs {
    from: String,
    question_id: String,
    answer: String,
    #[serde(default)]
    amend: bool,
}

#[derive(Debug, Deserialize)]
struct DeclineArgs {
    from: String,
    question_id: String,
    #[serde(default)]
    reason: Option<String>,
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    let (label, outcome) = match name {
        TOOL_ASK => {
            let args = parse_args_required::<AskArgs>(arguments)?;
            let body = json!({
                "from": args.from, "to": args.to, "question": args.question,
                "blocked": args.blocked, "about": args.about,
            });
            (
                "question",
                client.question_post("/api/questions", &body).await,
            )
        }
        TOOL_ANSWER => {
            let args = parse_args_required::<AnswerArgs>(arguments)?;
            let body = json!({"from": args.from, "answer": args.answer, "amend": args.amend});
            let path = format!("/api/questions/{}/answer", args.question_id.trim());
            ("answer", client.question_post(&path, &body).await)
        }
        TOOL_DECLINE => {
            let args = parse_args_required::<DeclineArgs>(arguments)?;
            let body = json!({"from": args.from, "reason": args.reason});
            let path = format!("/api/questions/{}/decline", args.question_id.trim());
            ("decline", client.question_post(&path, &body).await)
        }
        other => {
            return Err(RmcpError::invalid_params(
                format!("unknown question tool: {other}"),
                None,
            ));
        }
    };
    Ok(wrap_success(label, outcome.map_err(McpError::into_rmcp)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn question_tools_serve_every_name_with_descriptions_under_the_limit() {
        let tools = question_tools();
        assert_eq!(tools.len(), QUESTION_TOOL_NAMES.len());
        for tool in &tools {
            assert!(owns(tool.name.as_ref()));
            let length = tool.description.as_deref().map_or(0, str::len);
            assert!(length > 0 && length < 2048, "{}: {length}", tool.name);
        }
    }

    #[test]
    fn ask_requires_recipient_and_question() {
        let tool = ask_tool();
        let required = tool.input_schema.get("required").and_then(Value::as_array);
        let names: Vec<&str> = required
            .map(|r| r.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        assert_eq!(names, vec!["to", "question"]);
    }
}
