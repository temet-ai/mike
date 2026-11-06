use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde_json::{Map, Value, json};

use super::{arguments_as_value, object_schema, wrap_success};
use crate::client::InspectClient;
use crate::error::McpError;

pub const TOOL_FLEET_REPORT: &str = "fleet_report";
pub const TOOL_FLEET_VERDICT: &str = "fleet_verdict";
pub const FLEET_REPORT_TOOL_NAMES: &[&str] = &[TOOL_FLEET_REPORT, TOOL_FLEET_VERDICT];

pub fn fleet_report_tools() -> Vec<Tool> {
    vec![report_tool(), verdict_tool()]
}

pub fn owns(name: &str) -> bool {
    FLEET_REPORT_TOOL_NAMES.contains(&name)
}

fn strings(description: &str) -> Value {
    json!({"type": "array", "items": {"type": "string"}, "description": description})
}

fn criteria(description: &str) -> Value {
    json!({
        "anyOf": [
            {"type": "array", "items": {"type": "string", "minLength": 1}},
            {"type": "string", "minLength": 1}
        ],
        "description": description,
        "examples": [["C3", "C5"], "C3, C5"]
    })
}

fn text(description: &str) -> Value {
    json!({"type": "string", "description": description})
}

fn typed_report_fields() -> Vec<(&'static str, Value)> {
    vec![
        (
            "exercised",
            json!({
                "type": "object",
                "description": "The command you ran to exercise the changed behaviour. mike runs the task's declared floor at `sha` before any model reads this report.",
                "properties": {
                    "command": text("The command you ran."),
                    "exit": {"type": "integer", "description": "Its exit code."},
                    "output_ref": text("Where its output is kept: a file path or note id.")
                },
                "required": ["command", "exit", "output_ref"],
                "additionalProperties": false
            }),
        ),
        (
            "tests_changed",
            json!({
                "anyOf": [
                    {"type": "array", "items": {"type": "string", "minLength": 1}},
                    {"type": "string", "enum": ["none"]}
                ],
                "description": "Paths of every test, fixture or baseline you changed, or \"none\".",
                "examples": [["crates/mike/src/a_tests.rs"], "none"]
            }),
        ),
        (
            "unestablished",
            strings(
                "What you could not establish, one entry each, linked to its acceptance obligation; [] when nothing.",
            ),
        ),
    ]
}

fn report_tool() -> Tool {
    let schema = object_schema(
        vec![
            ("fleet", text("The fleet id or slug (get_fleet).")),
            (
                "session_id",
                text("Your own session id: the seat that holds the task."),
            ),
            ("task", text("The id of the task your seat holds.")),
            ("slice", text("A short name for the slice you finished.")),
            (
                "criteria",
                criteria("Task criterion ids: [\"C3\", \"C5\"] or \"C3, C5\". Default all."),
            ),
            (
                "summary",
                text(
                    "What you did and what it establishes: the commands you ran with their verdict lines, every test you changed and why, and what you could not establish. Kept as the note's body, not mailed.",
                ),
            ),
            (
                "look_at",
                text(
                    "How acceptance can see the outcome: the action, where, and what it should observe. Kept in the note, not mailed.",
                ),
            ),
            (
                "evidence",
                strings("Paths of your evidence: tests, result notes, files a check wrote."),
            ),
            (
                "sha",
                text(
                    "The commit to judge this report at: your fleet_commit sha, or a later one on the fleet's branch that contains it. `commit` is accepted as the same field.",
                ),
            ),
        ]
        .into_iter()
        .chain(typed_report_fields())
        .collect(),
        &[
            "fleet",
            "session_id",
            "task",
            "slice",
            "sha",
            "criteria",
            "exercised",
            "tests_changed",
            "unestablished",
        ],
    );
    Tool::new(
        Cow::Borrowed(TOOL_FLEET_REPORT),
        Cow::Borrowed(
            "Report a finished slice of your task, after fleet_commit. Mutating. Use it in \
             place of mailing acceptance. Typed: `sha` (or `commit`), `criteria`, \
             `exercised` {command, exit, output_ref}, `tests_changed` (paths or \"none\") and \
             `unestablished` ([] when nothing) are all required, and a report missing one is \
             refused naming the field. mike runs the task's declared floor at `sha`, writes a \
             `report` note in your notebook with what it finds itself: the commit, the \
             check's verdict at that commit, whether the check ran after your last change and \
             the brief version you read; and tells the judge the floor result and your \
             evidence paths, never your own words about the work. Also refused, naming the \
             problem, when your session does not hold the task, a criterion is not the \
             task's, or `slice` is empty. Returns `{report: {note, task, slice, criteria, commit, check, \
             checked_after_last_change, brief_version, evidence, told_acceptance, \
             acceptance_was_shown}}`: `acceptance_was_shown` is the mail text word for word. \
             The verdict comes back to you as a `[mike verdict]` message. `select` roots at \
             `/report`.",
        ),
        schema,
    )
}

fn verdict_tool() -> Tool {
    let schema = object_schema(
        vec![
            ("fleet", text("The fleet id or slug (get_fleet).")),
            (
                "session_id",
                text("Your own session id: the fleet's acceptance seat."),
            ),
            ("task", text("The id of the task judged.")),
            (
                "criteria",
                criteria("Judged criterion ids: [\"C3\", \"C5\"] or \"C3, C5\". Default all."),
            ),
            ("verdict", text("`pass` or `fail`.")),
            (
                "scope",
                text(
                    "`slice` for one slice, `final` for the criterion as a whole; only a final pass counts toward the criterion.",
                ),
            ),
            ("reason", text("Why, in words the seat can act on.")),
            (
                "regressions",
                json!({
                    "type": "array",
                    "description": "Regressions acceptance attributes to other tasks. Required failed obligations are saved before this verdict; free-text reasons never assign ownership.",
                    "items": {"type": "object", "additionalProperties": false,
                        "properties": {
                            "task": text("Other task responsible for the regression."),
                            "criteria": strings("Nonempty criterion ids belonging to that task."),
                            "reason": text("Observed regression and attribution."),
                            "failing_tests": strings("Failing test names."),
                            "evidence": strings("Evidence paths or note ids.")
                        },
                        "required": ["task", "criteria", "reason", "failing_tests", "evidence"]
                    }
                }),
            ),
            (
                "report_note",
                text("The note id fleet_report returned for the report you judged."),
            ),
        ],
        &["fleet", "session_id", "task", "verdict", "scope", "reason"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_FLEET_VERDICT),
        Cow::Borrowed(
            "Record acceptance's verdict on a task. Mutating. Only the fleet's acceptance seat \
             may call it. Use it in place of mailing a verdict or writing a decision note by \
             hand. mike writes the decision note in the shape the fleet's status and \
             acceptance register read (verdict, task, criteria, scope, reason, the report note \
             and the brief version that report was written against), records it in the change \
             record, and mails the task's seat once with the verdict and the reason. The \
             task's shown state follows from the notes: rework after a fail, passed when every \
             criterion has a final pass. Refused, naming the problem, when the caller is not \
             acceptance, the verdict or scope is another word, the reason is empty, a \
             criterion is not the task's, or `report_note` is not a report of that task. \
             Returns `{verdict: {note, task, verdict, scope, criteria, report_note, \
             brief_version, told_seat, seat_was_shown}}`. `select` roots at `/verdict`.",
        ),
        schema,
    )
}

fn fleet_route(fleet: &str, action: &str) -> Result<String, McpError> {
    let mut url = reqwest::Url::parse("http://x")
        .map_err(|e| McpError::Parse(format!("bad dummy base: {e}")))?;
    url.path_segments_mut()
        .map_err(|_| McpError::Parse("dummy url cannot be a base".to_string()))?
        .extend(["api", "fleets", fleet, action]);
    Ok(url.path().to_string())
}

fn body_of(arguments: Option<Map<String, Value>>) -> Result<(String, Value), RmcpError> {
    let mut body = arguments_as_value(arguments);
    let fields = body
        .as_object_mut()
        .ok_or_else(|| RmcpError::invalid_params("arguments must be an object", None))?;
    let fleet = fields
        .remove("fleet")
        .and_then(|v| v.as_str().map(str::to_string))
        .ok_or_else(|| {
            RmcpError::invalid_params("`fleet` is required: the fleet id or slug", None)
        })?;
    if let Some(session) = fields.remove("session_id") {
        fields.insert("session".to_string(), session);
    }
    Ok((fleet, body))
}

fn present(value: Option<&Value>) -> bool {
    match value {
        Some(Value::String(s)) => !s.trim().is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Null) | None => false,
        Some(_) => true,
    }
}

fn exercised_problem(exercised: Option<&Value>) -> Option<String> {
    const GIVE: &str = "give {command, exit, output_ref} (the command you ran, its exit code, and where its output is kept)";
    let Some(Value::Object(fields)) = exercised else {
        return Some(format!(
            "fleet_report is incomplete: `exercised` is missing; {GIVE}"
        ));
    };
    let missing = |key: &str| match key {
        "exit" => !fields.get(key).is_some_and(Value::is_i64),
        _ => !present(fields.get(key)),
    };
    ["command", "exit", "output_ref"]
        .into_iter()
        .find(|key| missing(key))
        .map(|key| format!("fleet_report is incomplete: `exercised.{key}` is missing; {GIVE}"))
}

fn report_incomplete(body: &Value) -> Option<String> {
    let field = |name: &str| body.get(name);
    let say = |name: &str, hint: &str| {
        Some(format!(
            "fleet_report is incomplete: `{name}` is missing; {hint}"
        ))
    };
    if !present(field("sha")) && !present(field("commit")) {
        return say("sha", "give the commit your fleet_commit returned");
    }
    if !present(field("criteria")) {
        return say("criteria", "give the task's criterion ids, e.g. [\"C3\"]");
    }
    if let Some(problem) = exercised_problem(field("exercised")) {
        return Some(problem);
    }
    if !present(field("tests_changed")) {
        return say(
            "tests_changed",
            "give the paths of every test you changed, or \"none\"",
        );
    }
    if !field("unestablished").is_some_and(Value::is_array) {
        return say(
            "unestablished",
            "give an array of what you could not establish; [] when nothing",
        );
    }
    None
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    let (action, key) = match name {
        TOOL_FLEET_REPORT => ("report", "report"),
        TOOL_FLEET_VERDICT => ("verdict", "verdict"),
        other => {
            return Err(RmcpError::invalid_params(
                format!("unknown fleet tool: {other}"),
                None,
            ));
        }
    };
    let (fleet, body) = body_of(arguments)?;
    if name == TOOL_FLEET_REPORT
        && let Some(problem) = report_incomplete(&body)
    {
        return Err(RmcpError::invalid_params(problem, None));
    }
    let path = fleet_route(&fleet, action).map_err(McpError::into_rmcp)?;
    let value = client
        .post_json(&path, &body)
        .await
        .map_err(McpError::into_rmcp)?;
    Ok(wrap_success(key, value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_tools_are_named_mutating_and_under_the_description_cap() {
        let tools = fleet_report_tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        assert_eq!(names, FLEET_REPORT_TOOL_NAMES);
        for tool in &tools {
            let text = tool.description.as_deref().unwrap_or_default();
            assert!(text.contains("Mutating."), "{}", tool.name);
            assert!(text.len() <= 2048, "{} is {} chars", tool.name, text.len());
        }
        assert!(owns(TOOL_FLEET_VERDICT) && !owns("fleet_commit"));
    }

    #[test]
    fn the_fleet_leaves_the_body_and_session_id_is_sent_as_session() -> anyhow::Result<()> {
        let mut args = Map::new();
        args.insert("fleet".into(), json!("change-record-3"));
        args.insert(
            "session_id".into(),
            json!("9dda3b31-f7e6-4547-9e13-33b566cfab97"),
        );
        args.insert("task".into(), json!("T4"));
        let (fleet, body) = body_of(Some(args)).map_err(|e| anyhow::anyhow!("{e:?}"))?;
        assert_eq!(fleet, "change-record-3");
        assert_eq!(
            body,
            json!({"session": "9dda3b31-f7e6-4547-9e13-33b566cfab97", "task": "T4"})
        );
        assert_eq!(fleet_route("a/b", "verdict")?, "/api/fleets/a%2Fb/verdict");
        Ok(())
    }

    fn complete_report() -> Value {
        json!({
            "sha": "abc123",
            "criteria": ["C3"],
            "exercised": {"command": "scripts/cx nextest run -p mike-mcp", "exit": 0, "output_ref": "note:1"},
            "tests_changed": "none",
            "unestablished": []
        })
    }

    #[test]
    fn a_complete_report_passes_and_commit_stands_for_sha() {
        assert_eq!(report_incomplete(&complete_report()), None);
        let mut body = complete_report();
        if let Some(fields) = body.as_object_mut() {
            let sha = fields.remove("sha");
            fields.insert("commit".into(), sha.unwrap_or_default());
        }
        assert_eq!(report_incomplete(&body), None);
    }

    #[test]
    fn a_report_missing_a_field_is_refused_naming_it() {
        for field in [
            "sha",
            "criteria",
            "exercised",
            "tests_changed",
            "unestablished",
        ] {
            let mut body = complete_report();
            if let Some(fields) = body.as_object_mut() {
                fields.remove(field);
            }
            let problem = report_incomplete(&body).unwrap_or_default();
            assert!(
                problem.starts_with(&format!("fleet_report is incomplete: `{field}`")),
                "{field}: {problem}"
            );
        }
    }

    #[test]
    fn a_report_names_the_missing_exercised_key() {
        for key in ["command", "exit", "output_ref"] {
            let mut body = complete_report();
            if let Some(exercised) = body["exercised"].as_object_mut() {
                exercised.remove(key);
            }
            let problem = report_incomplete(&body).unwrap_or_default();
            assert!(problem.contains(&format!("`exercised.{key}`")), "{problem}");
        }
    }

    #[test]
    fn report_schema_requires_the_typed_fields() {
        let tools = fleet_report_tools();
        let required = &tools[0].input_schema["required"];
        for field in [
            "sha",
            "criteria",
            "exercised",
            "tests_changed",
            "unestablished",
        ] {
            assert!(required.to_string().contains(field), "{field}");
        }
    }

    #[test]
    fn a_call_with_no_fleet_is_refused() {
        assert!(body_of(Some(Map::new())).is_err());
    }
    #[test]
    fn criteria_schema_advertises_both_supported_shapes() {
        for tool in fleet_report_tools() {
            let schema = &tool.input_schema["properties"]["criteria"];
            assert_eq!(schema["anyOf"][0]["type"], "array");
            assert_eq!(schema["anyOf"][1]["type"], "string");
            assert_eq!(schema["examples"][1], "C3, C5");
        }
    }
}
