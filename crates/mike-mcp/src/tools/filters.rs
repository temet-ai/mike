use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde_json::{Map, Value, json};

use super::{
    IdStringArgs, UpdateByIdArgs, arguments_as_value, id_only_string_schema, object_schema,
    parse_args_required, wrap_success,
};
use crate::client::InspectClient;
use crate::error::McpError;

pub const FILTERS_TOOL_NAMES: &[&str] = &[
    TOOL_LIST_FILTER_RULES,
    TOOL_CREATE_FILTER_RULE,
    TOOL_UPDATE_FILTER_RULE,
    TOOL_DELETE_FILTER_RULE,
    TOOL_DRY_RUN_FILTER_RULES,
];

pub const TOOL_LIST_FILTER_RULES: &str = "list_filter_rules";
pub const TOOL_CREATE_FILTER_RULE: &str = "create_filter_rule";
pub const TOOL_UPDATE_FILTER_RULE: &str = "update_filter_rule";
pub const TOOL_DELETE_FILTER_RULE: &str = "delete_filter_rule";
pub const TOOL_DRY_RUN_FILTER_RULES: &str = "dry_run_filter_rules";

pub fn filters_tools() -> Vec<Tool> {
    vec![
        list_filter_rules_tool(),
        create_filter_rule_tool(),
        update_filter_rule_tool(),
        delete_filter_rule_tool(),
        dry_run_filter_rules_tool(),
    ]
}

pub fn owns(name: &str) -> bool {
    FILTERS_TOOL_NAMES.contains(&name)
}

fn list_filter_rules_tool() -> Tool {
    Tool::new(
        Cow::Borrowed(TOOL_LIST_FILTER_RULES),
        Cow::Borrowed(
            "List every system-prompt filter rule in application order (`sort_order` ascending, \
             ties by `id`). Returns an array of \
             `{id, kind, value, action, text?, trigger_token?, enabled, source, updated_at, sort_order, title}`.",
        ),
        object_schema(vec![], &[]),
    )
}

fn create_filter_rule_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "kind",
                json!({
                    "type": "string",
                    "enum": ["text_substring", "xml_tag", "lazy_section", "inject_reminder"],
                    "description": "Match strategy. `lazy_section` strips a section from the system prompt and re-injects it as <system-reminder> when `trigger_token` appears in assistant text. `inject_reminder` injects a reminder once per window when `trigger_token` appears in assistant text and strips nothing (`value` is a label, `text` is the reminder body)."
                }),
            ),
            (
                "value",
                json!({
                    "type": "string",
                    "description": "Substring or tag name to match; for lazy_section, the markdown heading line (e.g. `# Claude in Chrome browser automation`); for inject_reminder, a human label (not a match target)."
                }),
            ),
            (
                "action",
                json!({
                    "type": "string",
                    "enum": ["drop", "replace", "append"],
                    "description": "Mutation applied when the rule matches. `lazy_section` and `inject_reminder` rules must use `replace`: `lazy_section` replaces the heading span with the stub in `text`; for `inject_reminder` the action has no other meaning."
                }),
            ),
            (
                "text",
                json!({
                    "type": "string",
                    "description": "Replacement / appended text. Required for `replace` and `append`; for `lazy_section`, the stub that replaces the stripped section; for `inject_reminder`, the reminder body to inject."
                }),
            ),
            (
                "trigger_token",
                json!({
                    "type": "string",
                    "description": "Required for lazy_section and inject_reminder: marker whose appearance in the latest assistant turn injects the reminder (lazy_section re-injects the stripped section; inject_reminder injects `text`)."
                }),
            ),
            (
                "inject_text",
                json!({
                    "type": "string",
                    "description": "Optional, for `lazy_section`: content injected when `trigger_token` fires, used instead of the section extracted from the system prompt."
                }),
            ),
            (
                "enabled",
                json!({
                    "type": "boolean",
                    "description": "Whether the rule is active. Default true."
                }),
            ),
            (
                "project_path",
                json!({
                    "type": "string",
                    "description": "Optional project scope. Omit (or null) for a global rule; set to an exact project path to fire only for that project."
                }),
            ),
            (
                "style_override",
                json!({
                    "type": "boolean",
                    "description": "Set true to save this rule although its `text` or `inject_text` contains vocabulary that `~/.claude/rules/response-style.md` bans, for example after a 400 naming the phrase. Stored on the rule, so later saves do not need it. Default false."
                }),
            ),
            (
                "title",
                json!({
                    "type": "string",
                    "description": "Optional operator-authored display name for the rule. Omit (or send an empty string) to leave it unset; the UI then derives a fallback title from kind/value/action."
                }),
            ),
        ],
        &["kind", "value", "action"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_CREATE_FILTER_RULE),
        Cow::Borrowed(
            "Create a filter rule. The server assigns `id` and `updated_at`. Returns \
             `{rule, warnings}`. 400 on validator rejection. Duplicate `(kind, value)` pairs are \
             permitted; ties in application order break on `id`.",
        ),
        schema,
    )
}

fn update_filter_rule_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "id",
                json!({
                    "type": "string",
                    "description": "Filter rule id."
                }),
            ),
            (
                "kind",
                json!({
                    "type": "string",
                    "enum": ["text_substring", "xml_tag", "lazy_section", "inject_reminder"],
                }),
            ),
            ("value", json!({ "type": "string" })),
            (
                "action",
                json!({
                    "type": "string",
                    "enum": ["drop", "replace", "append"],
                }),
            ),
            (
                "text",
                json!({
                    "type": ["string", "null"],
                    "description": "Send `null` to clear text on a `drop` rule; omit to leave unchanged."
                }),
            ),
            (
                "trigger_token",
                json!({
                    "type": ["string", "null"],
                    "description": "Trigger token for `lazy_section` / `inject_reminder` rules. Send `null` to clear (required when re-kinding away from those); omit to leave unchanged."
                }),
            ),
            (
                "inject_text",
                json!({
                    "type": ["string", "null"],
                    "description": "Override injection content for `lazy_section` rules. Send `null` to clear; omit to leave unchanged."
                }),
            ),
            ("enabled", json!({ "type": "boolean" })),
            (
                "project_path",
                json!({
                    "type": ["string", "null"],
                    "description": "Send null to clear project scope (make rule global); omit to leave unchanged."
                }),
            ),
            (
                "sort_order",
                json!({
                    "type": "integer",
                    "description": "New application order (ascending; ties break on id). Reordering rides this PATCH — set the moved rule(s)' sort_order. Omit to leave unchanged."
                }),
            ),
            (
                "style_override",
                json!({
                    "type": "boolean",
                    "description": "Set true to save this rule although its `text` or `inject_text` contains vocabulary that `~/.claude/rules/response-style.md` bans, for example after a 400 naming the phrase. Stored on the rule, so later saves do not need it. Omit to leave unchanged."
                }),
            ),
            (
                "title",
                json!({
                    "type": ["string", "null"],
                    "description": "Operator-authored display name. Send an empty string or null to clear it (falls back to a derived title); omit to leave unchanged."
                }),
            ),
        ],
        &["id"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_UPDATE_FILTER_RULE),
        Cow::Borrowed(
            "Patch fields on an existing filter rule. Absent keys leave the field unchanged. \
             Returns `{rule, warnings}`. 404 when id is unknown.",
        ),
        schema,
    )
}

fn delete_filter_rule_tool() -> Tool {
    Tool::new(
        Cow::Borrowed(TOOL_DELETE_FILTER_RULE),
        Cow::Borrowed(
            "Soft-delete a filter rule by id: the rule is marked disabled and its row stays. \
             Returns `{deleted: true, warnings: [...]}`. 404 when id is unknown.",
        ),
        id_only_string_schema(),
    )
}

fn dry_run_filter_rules_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "request_id",
                json!({
                    "type": "integer",
                    "minimum": 0,
                    "description": "Captured request whose ORIGINAL body is the dry-run input."
                }),
            ),
            (
                "rules",
                json!({
                    "type": "array",
                    "description": "Optional draft rules (same shape as create_filter_rule). Runs INSTEAD of active ruleset; never persisted. Omit to test current active ruleset.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "kind": { "type": "string", "enum": ["text_substring", "xml_tag", "lazy_section", "inject_reminder"] },
                            "value": { "type": "string" },
                            "action": { "type": "string", "enum": ["drop", "replace", "append"] },
                            "text": { "type": "string" },
                            "trigger_token": { "type": "string" },
                            "enabled": { "type": "boolean" },
                            "project_path": { "type": "string" }
                        },
                        "required": ["kind", "value", "action"]
                    }
                }),
            ),
        ],
        &["request_id"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_DRY_RUN_FILTER_RULES),
        Cow::Borrowed(
            "Run the production filter transform against a captured request and report what it \
             would change. Read-only. Returns `{request_id, changed, firing_rule_ids, \
             filtered_system, filtered_messages}`.",
        ),
        schema,
    )
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    match name {
        TOOL_LIST_FILTER_RULES => {
            let value = client
                .list_filter_rules()
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("rules", value))
        }
        TOOL_CREATE_FILTER_RULE => {
            let body = arguments_as_value(arguments);
            let value = client
                .create_filter_rule(body)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("rule", value))
        }
        TOOL_UPDATE_FILTER_RULE => {
            let args = parse_args_required::<UpdateByIdArgs>(arguments)?;
            let value = client
                .update_filter_rule(&args.id, Value::Object(args.rest))
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("rule", value))
        }
        TOOL_DELETE_FILTER_RULE => {
            let args = parse_args_required::<IdStringArgs>(arguments)?;
            let value = client
                .delete_filter_rule(&args.id)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("result", value))
        }
        TOOL_DRY_RUN_FILTER_RULES => {
            let body = arguments_as_value(arguments);
            let value = client
                .dry_run_filter_rules(body)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("dry_run", value))
        }
        other => Err(RmcpError::invalid_params(
            format!("unknown filter tool: {other}"),
            None,
        )),
    }
}
