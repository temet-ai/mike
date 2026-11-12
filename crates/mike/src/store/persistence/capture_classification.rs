use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};

pub(crate) fn session_prompt_content_sha256(blocks: &JsonValue) -> Option<String> {
    let array = blocks.as_array()?;
    let mut hasher = Sha256::new();
    for (idx, block) in array.iter().enumerate() {
        let block_index = idx.min(u8::MAX as usize) as u8;
        let text = block
            .get("text")
            .and_then(JsonValue::as_str)
            .unwrap_or_default();
        let stripped =
            crate::compose_prompt::volatile::strip_volatile_lines(text, &[], block_index);
        hasher.update(stripped.as_bytes());
        hasher.update([0x1e]);
    }
    Some(hex::encode(hasher.finalize()))
}

pub(crate) fn classify_author_kind(
    turn_origin: &str,
    human_prompt: &str,
    last_user_message: &str,
    is_tool_loop: bool,
    has_agent_id: bool,
) -> &'static str {
    match turn_origin {
        "subagent" | "workflow_subagent" => "agent",
        "title_sidecar"
        | "advisor_sidecar"
        | "quota_probe"
        | "monitor_sidecar"
        | "count_tokens_sidecar"
        | "web_fetch_sidecar"
        | "next_input_suggestion_sidecar" => "harness",
        "primary" | "primary_first" if has_agent_id => "agent",
        "primary" | "primary_first" => {
            classify_primary_turn_author(human_prompt, last_user_message, is_tool_loop)
        }
        _ => "unknown",
    }
}

fn is_mike_authored_followup(human_prompt: &str) -> bool {
    use crate::wire::genuine::markers;
    let trimmed = human_prompt.trim_start();
    trimmed.starts_with(markers::PROXY_TOOL_RESULT)
        || trimmed.starts_with(markers::WEASEL_CORRECTION)
}

fn machine_text_author_kind(kind: crate::wire::machine_text::MachineTextKind) -> &'static str {
    use crate::wire::machine_text::MachineTextKind as M;
    match kind {
        M::MailboxDelivery | M::TeammateMessage => "agent",
        _ => "harness",
    }
}

fn classify_primary_turn_author(
    human_prompt: &str,
    last_user_message: &str,
    is_tool_loop: bool,
) -> &'static str {
    if is_mike_authored_followup(human_prompt) {
        return "harness";
    }
    if let Some(kind) = crate::wire::machine_text::classify(human_prompt) {
        return machine_text_author_kind(kind);
    }
    if !human_prompt.trim().is_empty() {
        return "operator";
    }
    if let Some(kind) = crate::wire::machine_text::classify(last_user_message) {
        return machine_text_author_kind(kind);
    }
    if !last_user_message.trim().is_empty() {
        return "harness";
    }
    if is_tool_loop {
        return "harness";
    }
    "unknown"
}

pub(crate) fn extract_system_blocks_from_body(body: &[u8]) -> Option<JsonValue> {
    serde_json::from_slice::<JsonValue>(body)
        .ok()
        .and_then(|v| v.get("system").cloned())
        .filter(JsonValue::is_array)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(origin: &str, prompt: &str) -> &'static str {
        classify_author_kind(origin, prompt, prompt, false, false)
    }

    fn assert_unambiguous_origins_classify_without_content() {
        assert_eq!(typed("subagent", "anything"), "agent");
        assert_eq!(typed("workflow_subagent", "anything"), "agent");
        assert_eq!(typed("title_sidecar", "anything"), "harness");
        assert_eq!(typed("advisor_sidecar", "anything"), "harness");
        assert_eq!(typed("quota_probe", "anything"), "harness");
        assert_eq!(typed("monitor_sidecar", "anything"), "harness");
        assert_eq!(typed("count_tokens_sidecar", "anything"), "harness");
        assert_eq!(typed("web_fetch_sidecar", "anything"), "harness");
        assert_eq!(
            typed("next_input_suggestion_sidecar", "anything"),
            "harness"
        );
    }

    fn assert_ambiguous_primary_lane_decided_by_content() {
        assert_eq!(
            typed("primary", "please add a retry to the http client"),
            "operator"
        );
        assert_eq!(
            typed("primary_first", "please add a retry to the http client"),
            "operator"
        );
        assert_eq!(
            typed(
                "primary",
                "<teammate-message teammate_id=\"x\" summary=\"y\">hi</teammate-message>"
            ),
            "agent"
        );
        assert_eq!(
            typed("primary", "<agent-message from=\"y\">hi</agent-message>"),
            "agent"
        );
        assert_eq!(
            typed("primary", "Stop hook feedback:\nlint clean"),
            "harness"
        );
    }

    fn assert_nothing_to_attribute_is_unknown() {
        assert_eq!(
            classify_author_kind("primary", "", "", false, false),
            "unknown"
        );
        assert_eq!(
            classify_author_kind("primary", "   ", "   ", false, false),
            "unknown"
        );
    }

    fn assert_unrecognised_origin_falls_back_to_unknown() {
        assert_eq!(
            classify_author_kind("some_future_origin", "anything", "anything", false, false),
            "unknown"
        );
    }

    #[test]
    fn classify_author_kind_reads_mikes_own_followups_as_harness() {
        assert_eq!(
            typed("primary", "proxy tool result for 'mike_session_roster'"),
            "harness"
        );
        assert_eq!(
            typed("primary_first", "weasel correction for 'probably'"),
            "harness"
        );
        assert_eq!(
            typed("primary", "what is the proxy tool result for this?"),
            "operator"
        );
    }

    #[test]
    fn classify_author_kind_covers_every_branch() {
        assert_unambiguous_origins_classify_without_content();
        assert_ambiguous_primary_lane_decided_by_content();
        assert_nothing_to_attribute_is_unknown();
        assert_unrecognised_origin_falls_back_to_unknown();
    }

    #[test]
    fn classify_author_kind_reads_a_tool_result_continuation_as_harness() {
        assert_eq!(
            classify_author_kind("primary", "", "", true, false),
            "harness"
        );
        assert_eq!(
            classify_author_kind("primary_first", "", "", true, false),
            "harness"
        );
    }

    #[test]
    fn classify_author_kind_recovers_authorship_from_the_unstripped_message() {
        let last_user_message = concat!(
            "<system-reminder>\nNote: MEMORY.md changed on disk since you last read it.\n",
            "</system-reminder>\n<system-reminder>\n",
            "[SYSTEM NOTIFICATION - NOT USER INPUT]\n",
            "This is an automated background-task event, NOT a message from the user.\n",
            "<task-notification>\n<task-id>b78p1sf52</task-id>\n<status>completed</status>\n",
            "</task-notification>\n</system-reminder>",
        );
        assert_eq!(
            classify_author_kind("primary", "", last_user_message, false, false),
            "harness"
        );

        assert_eq!(
            classify_author_kind(
                "primary",
                "",
                "<system-reminder>Cross-session message from 8e3ad883</system-reminder>",
                false,
                false
            ),
            "agent"
        );

        assert_eq!(
            classify_author_kind(
                "primary",
                "",
                "<some-future-harness-envelope>x</some-future-harness-envelope>",
                false,
                false
            ),
            "harness"
        );
    }

    #[test]
    fn classify_author_kind_prefers_the_typed_prompt_over_its_envelope() {
        assert_eq!(
            classify_author_kind(
                "primary",
                "fix the retry backoff",
                "fix the retry backoff\n<system-reminder>MEMORY.md changed</system-reminder>",
                false,
                false
            ),
            "operator"
        );
        assert_eq!(
            classify_author_kind(
                "primary",
                "stop, do the other one first",
                "stop, do the other one first",
                true,
                false
            ),
            "operator"
        );
    }

    #[test]
    fn session_prompt_content_sha256_is_deterministic_hex() -> anyhow::Result<()> {
        let blocks = serde_json::json!([
            {"type": "text", "text": "billing header"},
            {"type": "text", "text": "role body"},
        ]);
        let a = session_prompt_content_sha256(&blocks)
            .ok_or_else(|| anyhow::anyhow!("array input must hash"))?;
        let b = session_prompt_content_sha256(&blocks)
            .ok_or_else(|| anyhow::anyhow!("array input must hash"))?;
        assert_eq!(a, b, "hashing the same blocks twice must be deterministic");
        assert_eq!(a.len(), 64, "sha256 hex must be 64 chars");
        assert!(
            a.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
            "must be lowercase hex"
        );

        let other = serde_json::json!([
            {"type": "text", "text": "billing header"},
            {"type": "text", "text": "DIFFERENT role body"},
        ]);
        let c = session_prompt_content_sha256(&other)
            .ok_or_else(|| anyhow::anyhow!("array input must hash"))?;
        assert_ne!(a, c, "distinct block text must produce distinct hashes");

        assert_eq!(
            session_prompt_content_sha256(&serde_json::json!("not an array")),
            None,
            "non-array blocks value must return None"
        );
        Ok(())
    }

    #[test]
    fn extract_system_blocks_from_body_finds_array_ignores_string_and_missing() {
        let with_array = br#"{"system":[{"type":"text","text":"a"}],"messages":[]}"#;
        assert!(extract_system_blocks_from_body(with_array).is_some());

        let string_form = br#"{"system":"plain string form","messages":[]}"#;
        assert_eq!(
            extract_system_blocks_from_body(string_form),
            None,
            "string-form system must not be treated as the block array"
        );

        let missing = br#"{"messages":[]}"#;
        assert_eq!(extract_system_blocks_from_body(missing), None);

        let not_json = b"not json at all";
        assert_eq!(extract_system_blocks_from_body(not_json), None);
    }

    #[test]
    fn classify_author_kind_reads_a_primary_lane_agent_id_request_as_agent() {
        assert_eq!(
            classify_author_kind("primary", "fix it", "fix it", false, true),
            "agent"
        );
        assert_eq!(
            classify_author_kind("primary_first", "", "", true, true),
            "agent"
        );
    }
}
