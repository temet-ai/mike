use super::*;
use serde_json::json;

use super::super::extract_turn_type;

#[test]
fn turn_origin_subagent_claude_code_header_with_persona_override() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "Implement the frontend half of the plan"}
        ]
    });
    let sp = "You are Claude Code, Anthropic's official CLI for Claude.\nYou are a Frontend Development Expert — a senior engineer who writes production-grade Svelte and TypeScript.";
    let origin = extract_turn_origin(
        &body,
        Some(sp),
        Some("claude-opus-4-7"),
        &TurnType::UserPrompt,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::Subagent);
    assert_eq!(origin.to_string(), "subagent");
}

#[test]
fn turn_origin_subagent_variant_with_an_persona() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "Audit this module"}
        ]
    });
    let sp = "You are Claude Code, Anthropic's official CLI for Claude.\nYou are an Expert Rust Reviewer with a deep background in async systems.";
    let origin = extract_turn_origin(
        &body,
        Some(sp),
        Some("claude-opus-4-7"),
        &TurnType::UserPrompt,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::Subagent);
}

#[test]
fn turn_origin_primary_when_persona_marker_but_session_identity_unestablished() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "Design the schema for the new table"}
        ]
    });
    let sp = "You are Claude Code, Anthropic's official CLI for Claude.\nYou are a Postgres + Rust Database Specialist — a senior data engineer who designs schemas that survive contact with production traffic.";
    let origin = extract_turn_origin(
        &body,
        Some(sp),
        Some("claude-opus-4-8"),
        &TurnType::UserPrompt,
        false,
        None,
    );
    assert_eq!(
        origin,
        TurnOrigin::Primary,
        "no established identity yet for this session — the persona request becomes its own identity, not a nested subagent"
    );
}

#[test]
fn turn_origin_primary_when_persona_matches_established_session_identity() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "..."},
            {
                "role": "assistant",
                "content": [{"type": "tool_use", "id": "t1", "name": "Read", "input": {}}]
            },
            {
                "role": "user",
                "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "..."}]
            }
        ]
    });
    let sp = "You are Claude Code, Anthropic's official CLI for Claude.\nYou are a Rust Development Expert -- a senior engineer who writes production-grade, idiomatic, and type-safe Rust code.";
    let origin = extract_turn_origin(
        &body,
        Some(sp),
        Some("claude-opus-4-8"),
        &TurnType::ToolLoop,
        true,
        Some("Rust Development Expert"),
    );
    assert_eq!(
        origin,
        TurnOrigin::Primary,
        "persona matches this session's own established identity — a continuation, not a nested dispatch"
    );
}

#[test]
fn turn_origin_subagent_when_persona_differs_from_established_session_identity() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "Verify this fix actually works"}
        ]
    });
    let sp = "You are Claude Code, Anthropic's official CLI for Claude.\nYou are a Verification Specialist — you prove things actually work.";
    let origin = extract_turn_origin(
        &body,
        Some(sp),
        Some("claude-opus-4-8"),
        &TurnType::UserPrompt,
        true,
        Some("Rust Development Expert"),
    );
    assert_eq!(
        origin,
        TurnOrigin::Subagent,
        "persona differs from this session's own established identity — a genuine nested dispatch"
    );
}

#[test]
fn subagent_persona_extracts_ui_designer_role_label() {
    let sp = "x-anthropic-billing-header: cc_version=2.1.121.d63;\nYou are Claude Code, Anthropic's official CLI for Claude.\nYou are a UI/design-system specialist. Your job is to extract, document, and codify visual design systems from reference sources.";
    assert_eq!(
        extract_subagent_persona(sp).as_deref(),
        Some("UI/design-system specialist")
    );
}

#[test]
fn subagent_persona_extracts_an_variant() {
    let sp = "You are Claude Code, Anthropic's official CLI for Claude.\nYou are an Expert Rust Reviewer with deep async-systems background. ...";
    assert_eq!(
        extract_subagent_persona(sp).as_deref(),
        Some("Expert Rust Reviewer with deep async-systems background")
    );
}

#[test]
fn subagent_persona_strips_em_dash_descriptor() {
    let sp = "You are a Frontend Development Expert — a senior engineer who builds production-grade UIs with Svelte 5, SvelteKit, and TypeScript.";
    assert_eq!(
        extract_subagent_persona(sp).as_deref(),
        Some("Frontend Development Expert")
    );
}

#[test]
fn subagent_persona_strips_double_hyphen_descriptor() {
    let sp = "You are a Rust Development Expert -- a senior engineer who writes production-grade, idiomatic, and type-safe Rust code.";
    assert_eq!(
        extract_subagent_persona(sp).as_deref(),
        Some("Rust Development Expert")
    );
}

#[test]
fn subagent_persona_returns_none_for_main_interactive_agent() {
    let sp = "You are Claude Code, Anthropic's official CLI for Claude.\nYou are an interactive agent that helps users with software engineering tasks.";
    assert_eq!(extract_subagent_persona(sp), None);
}

#[test]
fn subagent_persona_returns_none_when_no_override_present() {
    let sp = "You are Claude Code, Anthropic's official CLI for Claude.";
    assert_eq!(extract_subagent_persona(sp), None);
}

#[test]
fn subagent_persona_returns_none_for_empty_prompt() {
    assert_eq!(extract_subagent_persona(""), None);
}

#[test]
fn subagent_persona_returns_full_long_label_without_truncating() {
    let huge = "x".repeat(200);
    let sp = format!(
        "You are Claude Code, Anthropic's official CLI for Claude.\nYou are a {huge} that ought to fit in a badge"
    );
    let expected = format!("{huge} that ought to fit in a badge");
    assert_eq!(
        extract_subagent_persona(&sp).as_deref(),
        Some(expected.as_str())
    );
}

#[test]
fn subagent_persona_skips_main_line_and_picks_following_override() {
    let sp = "You are Claude Code, Anthropic's official CLI for Claude.\nYou are an interactive agent that helps users with software engineering tasks.\nYou are a Frontend Development Expert.";
    assert_eq!(
        extract_subagent_persona(sp).as_deref(),
        Some("Frontend Development Expert")
    );
}

#[test]
fn subagent_persona_skips_agent_sdk_preamble_and_reads_a_the_definition() {
    let sp = "x-anthropic-billing-header: cc_version=2.1.311.7ca; cc_entrypoint=sdk-cli; cc_is_subagent=true;\nYou are a Claude agent, built on Anthropic's Claude Agent SDK.\nYou are the database specialist for mike's capture database. You own the shape of the data.";
    assert_eq!(
        extract_subagent_persona(sp).as_deref(),
        Some("database specialist for mike's capture database")
    );
}

#[test]
fn subagent_persona_returns_none_for_agent_sdk_preamble_alone() {
    let sp = "You are a Claude agent, built on Anthropic's Claude Agent SDK.";
    assert_eq!(extract_subagent_persona(sp), None);
}

#[test]
fn turn_origin_primary_for_main_claude_code_system_prompt() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "hello"}
        ]
    });
    let sp = "You are Claude Code, Anthropic's official CLI for Claude.\nYou are an interactive agent that helps users with software engineering tasks.";
    let origin = extract_turn_origin(
        &body,
        Some(sp),
        Some("claude-opus-4-7"),
        &TurnType::UserPrompt,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::Primary);
    assert_eq!(origin.to_string(), "primary");
}

#[test]
fn turn_origin_subagent_when_message_count_gt_one_with_persona_marker() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "first turn"},
            {"role": "assistant", "content": "response"},
            {"role": "user", "content": "follow-up"}
        ]
    });
    let sp = "You are Claude Code, Anthropic's official CLI for Claude.\nYou are a Frontend Development Expert.";
    let origin = extract_turn_origin(
        &body,
        Some(sp),
        Some("claude-opus-4-7"),
        &TurnType::UserPrompt,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::Subagent);
}

#[test]
fn turn_origin_workflow_subagent_multi_message_with_marker() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "first"},
            {"role": "assistant", "content": "response"},
            {"role": "user", "content": "follow-up"}
        ]
    });
    let sp = "You are Claude Code, Anthropic's official CLI for Claude.\n\
              You are running inside a workflow script. The current working directory is /app.";
    let origin = extract_turn_origin(
        &body,
        Some(sp),
        Some("claude-sonnet-4-6"),
        &TurnType::UserPrompt,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::WorkflowSubagent);
    assert_eq!(origin.to_string(), "workflow_subagent");
}

#[test]
fn turn_origin_subagent_not_reclassified_as_workflow_when_no_marker() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "do the task"}
        ]
    });
    let sp = "You are Claude Code, Anthropic's official CLI for Claude.\n\
              You are a Rust Development Expert.";
    let origin = extract_turn_origin(
        &body,
        Some(sp),
        Some("claude-sonnet-4-6"),
        &TurnType::UserPrompt,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::Subagent);
    assert_ne!(origin, TurnOrigin::WorkflowSubagent);
}

#[test]
fn turn_origin_workflow_subagent_tool_loop_with_marker() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "verify the fix"},
            {"role": "assistant", "content": [{"type": "tool_use", "id": "t1", "name": "Read", "input": {}}]},
            {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "file contents"}]}
        ]
    });
    let sp = "You are Claude Code, Anthropic's official CLI for Claude.\n\
              You are a Verification Specialist.\n\
              You are running inside a workflow script. The current working directory is /app.";
    assert_eq!(extract_turn_type(&body), TurnType::ToolLoop);
    let origin = extract_turn_origin(
        &body,
        Some(sp),
        Some("claude-sonnet-4-6"),
        &TurnType::ToolLoop,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::WorkflowSubagent);
}

#[test]
fn turn_origin_subagent_tool_loop_with_persona_override() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "review this function"},
            {"role": "assistant", "content": [{"type": "tool_use", "id": "t1", "name": "Read", "input": {}}]},
            {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "file contents"}]}
        ]
    });
    let sp = "You are Claude Code, Anthropic's official CLI for Claude.\n\
              You are a Rust Development Expert.";
    assert_eq!(extract_turn_type(&body), TurnType::ToolLoop);
    let origin = extract_turn_origin(
        &body,
        Some(sp),
        Some("claude-sonnet-4-6"),
        &TurnType::ToolLoop,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::Subagent);
}

#[test]
fn turn_origin_subagent_for_thread_mode_billing_header_only_system_prompt() {
    let body = json!({
        "messages": [
            {"role": "user", "content": [
                {"tool_use_id": "toolu_011Cf9QtvURXXrcgJrnJ2KKT", "type": "tool_result", "content": "The file has been updated successfully."}
            ]},
            {"role": "system", "content": [
                {"type": "text", "text": "<system-reminder>\n<total_tokens>1000</total_tokens>\n</system-reminder>", "cache_control": {"type": "ephemeral"}}
            ]}
        ]
    });
    let sp = "x-anthropic-billing-header: cc_version=2.1.274.6d9; cc_entrypoint=cli; cch=93af5; cc_is_subagent=true; cc_prev_req=req_011Cf9QtvURXXrcgJrnJ2KKT; cc_prompt_id=abc123";
    let turn_type = extract_turn_type(&body);
    assert_eq!(turn_type, TurnType::ToolLoop);
    let origin = extract_turn_origin(
        &body,
        Some(sp),
        Some("claude-sonnet-4-6"),
        &turn_type,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::Subagent);
}

#[test]
fn turn_origin_primary_for_genuine_operator_prompt_no_tool_result() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "Refactor the session store to use the new schema."}
        ]
    });
    let sp = "x-anthropic-billing-header: cc_version=2.1.274.6d9; cc_entrypoint=cli; cch=93af5; cc_is_subagent=false; cc_prompt_id=abc123";
    let turn_type = extract_turn_type(&body);
    assert_eq!(turn_type, TurnType::UserPrompt);
    let origin = extract_turn_origin(
        &body,
        Some(sp),
        Some("claude-sonnet-4-6"),
        &turn_type,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::Primary);
}

#[test]
fn turn_origin_primary_for_plain_tool_loop_without_marker() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "fix the bug"},
            {"role": "assistant", "content": [{"type": "tool_use", "id": "t1", "name": "Read", "input": {}}]},
            {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "file contents"}]}
        ]
    });
    let sp = "You are Claude Code, Anthropic's official CLI for Claude.\n\
              You are an interactive agent that helps users with software engineering tasks.";
    assert_eq!(extract_turn_type(&body), TurnType::ToolLoop);
    let origin = extract_turn_origin(
        &body,
        Some(sp),
        Some("claude-sonnet-4-6"),
        &TurnType::ToolLoop,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::Primary);
}
