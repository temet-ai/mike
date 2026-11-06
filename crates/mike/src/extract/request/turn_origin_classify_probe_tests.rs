use super::*;
use serde_json::json;

#[test]
fn turn_origin_quota_probe_haiku_one_token_literal_quota() {
    let body = json!({
        "model": "claude-haiku-4-5-20251001",
        "max_tokens": 1,
        "messages": [
            {"role": "user", "content": "quota"}
        ]
    });
    let origin = extract_turn_origin(
        &body,
        None,
        Some("claude-haiku-4-5-20251001"),
        &TurnType::UserPrompt,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::QuotaProbe);
    assert_eq!(origin.to_string(), "quota_probe");
}

#[test]
fn turn_origin_primary_when_haiku_one_token_but_content_not_quota() {
    let body = json!({
        "max_tokens": 1,
        "messages": [
            {"role": "user", "content": "hello"}
        ]
    });
    let origin = extract_turn_origin(
        &body,
        None,
        Some("claude-haiku-4-5-20251001"),
        &TurnType::UserPrompt,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::Primary);
}

#[test]
fn turn_origin_primary_when_quota_content_but_max_tokens_not_one() {
    let body = json!({
        "max_tokens": 1024,
        "messages": [
            {"role": "user", "content": "quota"}
        ]
    });
    let origin = extract_turn_origin(
        &body,
        None,
        Some("claude-haiku-4-5-20251001"),
        &TurnType::UserPrompt,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::Primary);
}

#[test]
fn turn_origin_quota_probe_on_non_haiku_model() {
    let body = json!({
        "max_tokens": 1,
        "messages": [
            {"role": "user", "content": "quota"}
        ]
    });
    let origin = extract_turn_origin(
        &body,
        None,
        Some("claude-opus-4-7"),
        &TurnType::UserPrompt,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::QuotaProbe);
}

#[test]
fn turn_origin_primary_when_quota_shape_but_content_is_block_array() {
    let body = json!({
        "max_tokens": 1,
        "messages": [
            {"role": "user", "content": [
                {"type": "text", "text": "quota"}
            ]}
        ]
    });
    let origin = extract_turn_origin(
        &body,
        None,
        Some("claude-haiku-4-5-20251001"),
        &TurnType::UserPrompt,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::Primary);
}

#[test]
fn turn_origin_quota_probe_on_opus_4_8_1m() {
    let body = json!({
        "model": "claude-opus-4-8[1m]",
        "max_tokens": 1,
        "messages": [
            {"role": "user", "content": "quota"}
        ]
    });
    let origin = extract_turn_origin(
        &body,
        None,
        Some("claude-opus-4-8[1m]"),
        &TurnType::UserPrompt,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::QuotaProbe);
    assert_eq!(origin.to_string(), "quota_probe");
}

#[test]
fn turn_origin_primary_for_real_opus_user_turn_not_false_positive() {
    let body = json!({
        "max_tokens": 32000,
        "messages": [
            {"role": "user", "content": "Refactor the session store to use the new schema."}
        ]
    });
    let sp = "You are Claude Code, Anthropic's official CLI for Claude.\nYou are an interactive agent that helps users with software engineering tasks.";
    let origin = extract_turn_origin(
        &body,
        Some(sp),
        Some("claude-opus-4-8"),
        &TurnType::UserPrompt,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::Primary);
}

#[test]
fn turn_origin_quota_probe_with_trailing_system_message() {
    let body = json!({
        "max_tokens": 1,
        "messages": [
            {"role": "user", "content": "quota"},
            {"role": "system", "content": "mid-session policy update"}
        ]
    });
    let origin = extract_turn_origin(
        &body,
        None,
        Some("claude-opus-4-8"),
        &TurnType::UserPrompt,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::QuotaProbe);
}

#[test]
fn turn_origin_web_fetch_sidecar_single_text_block() {
    let body = json!({
        "messages": [{
            "role": "user",
            "content": [{"type": "text", "text": "\nWeb page content:\n---\n# Prompt caching\n…page markdown…\n---\n\nSummarize the caching rules. - Never produce or reproduce exact song lyrics."}]
        }]
    });
    let origin = extract_turn_origin(
        &body,
        None,
        Some("claude-sonnet-4-6"),
        &TurnType::UserPrompt,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::WebFetchSidecar);
}

#[test]
fn turn_origin_web_fetch_sidecar_string_content() {
    let body = json!({
        "messages": [{
            "role": "user",
            "content": "Web page content:\n---\n<page>\n---\nquestion"
        }]
    });
    let origin = extract_turn_origin(&body, None, None, &TurnType::UserPrompt, true, None);
    assert_eq!(origin, TurnOrigin::WebFetchSidecar);
}

#[test]
fn turn_origin_primary_when_web_page_marker_not_leading() {
    let body = json!({
        "messages": [{
            "role": "user",
            "content": "Why does the proxy classify messages starting with\nWeb page content:\n--- as sidecars?"
        }]
    });
    let origin = extract_turn_origin(&body, None, None, &TurnType::UserPrompt, true, None);
    assert_eq!(origin, TurnOrigin::Primary);
}

#[test]
fn turn_origin_primary_when_web_page_marker_in_multi_message_convo() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "\nWeb page content:\n---\npasted page"},
            {"role": "assistant", "content": "noted"},
            {"role": "user", "content": "and?"}
        ]
    });
    let origin = extract_turn_origin(&body, None, None, &TurnType::UserPrompt, true, None);
    assert_eq!(origin, TurnOrigin::Primary);
}

#[test]
fn turn_origin_next_input_suggestion_sidecar_multi_message() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "fix the bug in parser.rs"},
            {"role": "assistant", "content": "done, fixed it"},
            {"role": "user", "content": "[SUGGESTION MODE: predict the operator's next likely message]"}
        ]
    });
    let origin = extract_turn_origin(
        &body,
        None,
        Some("claude-sonnet-4-6"),
        &TurnType::UserPrompt,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::NextInputSuggestionSidecar);
    assert_eq!(origin.to_string(), "next_input_suggestion_sidecar");
    assert!(!origin.stamps_capture_identity());
}

#[test]
fn turn_origin_primary_when_suggestion_marker_not_leading() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "first"},
            {"role": "assistant", "content": "response"},
            {"role": "user", "content": "why does the proxy special-case text starting with\n[SUGGESTION MODE: ?"}
        ]
    });
    let origin = extract_turn_origin(&body, None, None, &TurnType::UserPrompt, true, None);
    assert_eq!(origin, TurnOrigin::Primary);
}
