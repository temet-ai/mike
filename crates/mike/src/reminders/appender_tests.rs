use super::appender::{append_block_to_last_user_message, payload_present_in_last_user_message};
use crate::store::models::RequestId;
use serde_json::{Value, json};

#[test]
fn appends_block_to_last_user_message_with_array_content() -> anyhow::Result<()> {
    let body = json!({
        "model": "claude-sonnet-4-20250514",
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "first"}]},
            {"role": "assistant", "content": [{"type": "text", "text": "reply"}]},
            {"role": "user", "content": [{"type": "text", "text": "follow-up"}]},
        ],
    });
    let bytes = serde_json::to_vec(&body)?;

    let new_bytes = append_block_to_last_user_message(&bytes, "REMINDER", RequestId::next())
        .ok_or_else(|| anyhow::anyhow!("expected rewrite"))?;
    let new_body: Value = serde_json::from_slice(&new_bytes)?;
    let last_content = new_body["messages"][2]["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("content must be array"))?;
    assert_eq!(last_content.len(), 2, "original block + appended reminder");
    assert_eq!(last_content[0]["text"], "follow-up");
    assert_eq!(last_content[1]["type"], "text");
    assert_eq!(last_content[1]["text"], "REMINDER");
    let first_content = new_body["messages"][0]["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("content must be array"))?;
    assert_eq!(first_content.len(), 1);
    assert_eq!(first_content[0]["text"], "first");
    Ok(())
}

fn top_level_spans(body: &[u8]) -> anyhow::Result<std::collections::HashMap<String, String>> {
    let fields: std::collections::HashMap<&str, &serde_json::value::RawValue> =
        serde_json::from_slice(body)?;
    Ok(fields
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.get().to_string()))
        .collect())
}

#[test]
fn splice_preserves_byte_identity_of_untouched_top_level_keys() -> anyhow::Result<()> {
    let raw: &[u8] = br#"{"model":"m","system":[{"type":"text","text":"identity preamble"}],"tools":[{"zeta":1,"alpha":2,"input_schema":{"type":"object"}}],"context_management":{"zeta_opt":true,"alpha_opt":false},"messages":[{"role":"user","content":[{"type":"text","text":"hi"}]}]}"#;

    let new_bytes = append_block_to_last_user_message(raw, "REMINDER", RequestId::next())
        .ok_or_else(|| anyhow::anyhow!("expected rewrite"))?;

    let before = top_level_spans(raw)?;
    let after = top_level_spans(&new_bytes)?;
    for (key, before_bytes) in &before {
        if key == "messages" {
            continue;
        }
        let after_bytes = after
            .get(key)
            .ok_or_else(|| anyhow::anyhow!("key `{key}` missing from the rewritten body"))?;
        assert_eq!(
            before_bytes, after_bytes,
            "key `{key}` must stay byte-identical when only `messages` changed \
             (a whole-body reserialize would resort its keys — see issue #186)"
        );
    }

    let new_body: Value = serde_json::from_slice(&new_bytes)?;
    let content = new_body["messages"][0]["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("content must be array"))?;
    assert_eq!(content.len(), 2, "original block + appended reminder");
    assert_eq!(content[1]["type"], "text");
    assert_eq!(content[1]["text"], "REMINDER");
    Ok(())
}

#[test]
fn splice_preserves_key_order_of_tool_use_inputs_throughout_history() -> anyhow::Result<()> {
    let head: &[u8] = br#"{"model":"m","messages":[{"role":"user","content":[{"type":"text","text":"go"}]},{"role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"ToolSearch","input":{"zebra":1,"apple":2,"max_results":10}}]},{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"ok"},{"type":"text","text":"more please"}"#;
    let tail: &[u8] = br#"]}]}"#;
    let raw: Vec<u8> = [head, tail].concat();

    let new_bytes = append_block_to_last_user_message(&raw, "REMINDER", RequestId::next())
        .ok_or_else(|| anyhow::anyhow!("expected rewrite"))?;

    let inserted: &[u8] = br#",{"text":"REMINDER","type":"text"}"#;
    let expected: Vec<u8> = [head, inserted, tail].concat();
    assert_eq!(
        new_bytes, expected,
        "the byte-splice must equal the original bytes with the new block \
         inserted at exactly the content array's closing bracket — every \
         other byte, including the unsorted tool_use.input keys earlier in \
         the history, must be untouched"
    );

    let new_body: Value = serde_json::from_slice(&new_bytes)?;
    let last_content = new_body["messages"][2]["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("content must be array"))?;
    assert_eq!(
        last_content.len(),
        3,
        "two original blocks + appended reminder"
    );
    assert_eq!(last_content[2]["type"], "text");
    assert_eq!(last_content[2]["text"], "REMINDER");
    Ok(())
}

#[test]
fn last_message_assistant_role_returns_none() -> anyhow::Result<()> {
    let body = json!({
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "q"}]},
            {"role": "assistant", "content": [{"type": "text", "text": "a"}]},
        ],
    });
    let bytes = serde_json::to_vec(&body)?;
    assert!(append_block_to_last_user_message(&bytes, "X", RequestId::next()).is_none());
    Ok(())
}

#[test]
fn trailing_system_message_appends_to_last_user() -> anyhow::Result<()> {
    let body = json!({
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "q"}]},
            {"role": "assistant", "content": [{"type": "text", "text": "a"}]},
            {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "id1", "content": "ok"}]},
            {"role": "system", "content": "Proactive output style is active."},
        ],
    });
    let bytes = serde_json::to_vec(&body)?;
    let new_bytes = append_block_to_last_user_message(&bytes, "REMINDER", RequestId::next())
        .ok_or_else(|| {
            anyhow::anyhow!("expected rewrite — trailing system must not block inject")
        })?;
    let new_body: Value = serde_json::from_slice(&new_bytes)?;
    let content = new_body["messages"][2]["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("content must be array"))?;
    assert_eq!(
        content.len(),
        2,
        "original tool_result block + appended reminder"
    );
    assert_eq!(content[1]["type"], "text");
    assert_eq!(content[1]["text"], "REMINDER");
    assert_eq!(
        new_body["messages"][3]["role"], "system",
        "trailing system message must be preserved"
    );
    assert_eq!(
        new_body["messages"][3]["content"],
        "Proactive output style is active."
    );
    Ok(())
}

#[test]
fn trailing_system_with_subsequent_assistant_returns_none() -> anyhow::Result<()> {
    let body = json!({
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "q"}]},
            {"role": "assistant", "content": [{"type": "text", "text": "a"}]},
        ],
    });
    let bytes = serde_json::to_vec(&body)?;
    assert!(append_block_to_last_user_message(&bytes, "X", RequestId::next()).is_none());
    Ok(())
}

#[test]
fn missing_messages_returns_none() -> anyhow::Result<()> {
    let body = json!({"model": "claude"});
    let bytes = serde_json::to_vec(&body)?;
    assert!(append_block_to_last_user_message(&bytes, "X", RequestId::next()).is_none());
    Ok(())
}

#[test]
fn empty_messages_returns_none() -> anyhow::Result<()> {
    let body = json!({"messages": []});
    let bytes = serde_json::to_vec(&body)?;
    assert!(append_block_to_last_user_message(&bytes, "X", RequestId::next()).is_none());
    Ok(())
}

fn plain_string_body(trailing_system: bool) -> anyhow::Result<(Vec<u8>, usize)> {
    let mut messages = vec![
        json!({"role": "assistant", "content": [
            {"type": "tool_use", "id": "t1", "name": "Bash", "input": {"zeta": 1, "alpha": 2}}
        ]}),
        json!({"role": "user", "content": "plain \"quoted\" string\nwith \u{e9} text"}),
    ];
    if trailing_system {
        messages.push(json!({"role": "system", "content": "tail"}));
    }
    let bytes = serde_json::to_vec(&json!({"model": "m", "messages": messages}))?;
    let find = |marker: &[u8]| bytes.windows(marker.len()).position(|w| w == marker);
    let start = find(br#"{"content":"plain"#)
        .or_else(|| find(br#"{"role":"user","content":"plain"#))
        .ok_or_else(|| anyhow::anyhow!("marker missing"))?;
    Ok((bytes, start))
}

fn assert_plain_string_gains_block(trailing_system: bool) -> anyhow::Result<()> {
    let (bytes, user_start) = plain_string_body(trailing_system)?;
    let out = append_block_to_last_user_message(&bytes, "REMINDER", RequestId::next())
        .ok_or_else(|| anyhow::anyhow!("plain-string content must accept the block"))?;
    assert_eq!(out[..user_start], bytes[..user_start]);
    let parsed: Value = serde_json::from_slice(&out)?;
    let user_idx = 1;
    let content = parsed["messages"][user_idx]["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("content must now be a block list"))?;
    assert_eq!(content.len(), 2);
    assert_eq!(content[0]["type"], "text");
    assert_eq!(
        content[0]["text"],
        "plain \"quoted\" string\nwith \u{e9} text"
    );
    assert_eq!(content[1]["text"], "REMINDER");
    if trailing_system {
        assert_eq!(parsed["messages"][2]["content"], "tail");
    }
    assert!(payload_present_in_last_user_message(&out, "REMINDER"));
    Ok(())
}

#[test]
fn string_shaped_last_user_content_gains_block_with_prefix_unchanged() -> anyhow::Result<()> {
    assert_plain_string_gains_block(false)
}

#[test]
fn string_shaped_last_user_content_gains_block_before_trailing_system_message() -> anyhow::Result<()>
{
    assert_plain_string_gains_block(true)
}

#[test]
fn string_shaped_content_alone_gains_block() -> anyhow::Result<()> {
    let body = json!({
        "messages": [{"role": "user", "content": "hello"}],
    });
    let bytes = serde_json::to_vec(&body)?;
    let out = append_block_to_last_user_message(&bytes, "X", RequestId::next())
        .ok_or_else(|| anyhow::anyhow!("expected a rewrite"))?;
    let parsed: Value = serde_json::from_slice(&out)?;
    assert_eq!(parsed["messages"][0]["content"][0]["text"], "hello");
    assert_eq!(parsed["messages"][0]["content"][1]["text"], "X");
    Ok(())
}

#[test]
fn non_string_non_array_content_returns_none() -> anyhow::Result<()> {
    let bytes = serde_json::to_vec(&json!({
        "messages": [{"role": "user", "content": 7}],
    }))?;
    assert!(append_block_to_last_user_message(&bytes, "X", RequestId::next()).is_none());
    Ok(())
}

#[test]
fn missing_role_returns_none() -> anyhow::Result<()> {
    let body = json!({
        "messages": [{"content": [{"type": "text", "text": "q"}]}],
    });
    let bytes = serde_json::to_vec(&body)?;
    assert!(append_block_to_last_user_message(&bytes, "X", RequestId::next()).is_none());
    Ok(())
}

#[test]
fn invalid_json_returns_none() {
    assert!(append_block_to_last_user_message(b"not json {{{", "X", RequestId::next()).is_none());
}

#[test]
fn top_level_array_returns_none() -> anyhow::Result<()> {
    let bytes = serde_json::to_vec(&json!([1, 2, 3]))?;
    assert!(append_block_to_last_user_message(&bytes, "X", RequestId::next()).is_none());
    Ok(())
}

#[test]
fn appended_block_has_no_cache_control() -> anyhow::Result<()> {
    let body = json!({
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": "q",
                 "cache_control": {"type": "ephemeral"}}
            ]
        }],
    });
    let bytes = serde_json::to_vec(&body)?;
    let new_bytes = append_block_to_last_user_message(&bytes, "REMIND", RequestId::next())
        .ok_or_else(|| anyhow::anyhow!("expected rewrite"))?;
    let new_body: Value = serde_json::from_slice(&new_bytes)?;
    let arr = new_body["messages"][0]["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("content must be array"))?;
    assert_eq!(arr[0]["cache_control"]["type"], "ephemeral");
    assert!(arr[1].get("cache_control").is_none());
    Ok(())
}

#[test]
fn long_reminder_text_round_trips_verbatim() -> anyhow::Result<()> {
    let body = json!({
        "messages": [{"role": "user", "content": [{"type": "text", "text": "q"}]}],
    });
    let bytes = serde_json::to_vec(&body)?;
    let huge: String = "X".repeat(8_000);
    let new_bytes = append_block_to_last_user_message(&bytes, &huge, RequestId::next())
        .ok_or_else(|| anyhow::anyhow!("expected rewrite"))?;
    let new_body: Value = serde_json::from_slice(&new_bytes)?;
    assert_eq!(new_body["messages"][0]["content"][1]["text"], huge);
    Ok(())
}

#[test]
fn presence_check_true_after_real_append() -> anyhow::Result<()> {
    let body = json!({
        "messages": [{"role": "user", "content": [{"type": "text", "text": "q"}]}],
    });
    let bytes = serde_json::to_vec(&body)?;
    let payload = "<system-reminder>finish first</system-reminder>";
    let new_bytes = append_block_to_last_user_message(&bytes, payload, RequestId::next())
        .ok_or_else(|| anyhow::anyhow!("expected rewrite"))?;
    assert!(
        payload_present_in_last_user_message(&new_bytes, payload),
        "payload appended by the same function must be detected present"
    );
    Ok(())
}

#[test]
fn presence_check_survives_json_escaping_of_newlines_and_quotes() -> anyhow::Result<()> {
    let body = json!({
        "messages": [{"role": "user", "content": [{"type": "text", "text": "q"}]}],
    });
    let bytes = serde_json::to_vec(&body)?;
    let payload = "<system-reminder>Automated steering note:\nsay \"hi\"\n\nAddress this note.</system-reminder>";
    let new_bytes = append_block_to_last_user_message(&bytes, payload, RequestId::next())
        .ok_or_else(|| anyhow::anyhow!("expected rewrite"))?;
    assert!(
        !String::from_utf8_lossy(&new_bytes).contains(payload),
        "wire bytes escape the payload; raw substring would be a false negative"
    );
    assert!(
        payload_present_in_last_user_message(&new_bytes, payload),
        "JSON-aware check must confirm the escaped payload is present"
    );
    Ok(())
}

#[test]
fn presence_check_false_when_inject_did_not_mutate_body() -> anyhow::Result<()> {
    let unmutated = json!({
        "messages": [{"role": "user", "content": [{"type": "text", "text": "q"}]}],
    });
    let bytes = serde_json::to_vec(&unmutated)?;
    assert!(
        !payload_present_in_last_user_message(&bytes, "<system-reminder>x</system-reminder>"),
        "payload absent from body must report not-present"
    );
    Ok(())
}

#[test]
fn presence_check_false_on_partial_substring_match() -> anyhow::Result<()> {
    let body = json!({
        "messages": [{"role": "user", "content": [
            {"type": "text", "text": "prefix <system-reminder>x</system-reminder> suffix"}
        ]}],
    });
    let bytes = serde_json::to_vec(&body)?;
    assert!(
        !payload_present_in_last_user_message(&bytes, "<system-reminder>x</system-reminder>"),
        "substring-only match must not count as delivered"
    );
    Ok(())
}

#[test]
fn presence_check_false_on_invalid_or_shapeless_body() -> anyhow::Result<()> {
    assert!(!payload_present_in_last_user_message(b"not json {{{", "X"));
    let no_messages = serde_json::to_vec(&json!({"model": "claude"}))?;
    assert!(!payload_present_in_last_user_message(&no_messages, "X"));
    let string_content = serde_json::to_vec(&json!({
        "messages": [{"role": "user", "content": "hello"}],
    }))?;
    assert!(!payload_present_in_last_user_message(&string_content, "X"));
    Ok(())
}

#[test]
fn empty_reminder_text_returns_none() -> anyhow::Result<()> {
    let body = json!({
        "messages": [{"role": "user", "content": [{"type": "text", "text": "q"}]}],
    });
    let bytes = serde_json::to_vec(&body)?;
    assert!(append_block_to_last_user_message(&bytes, "   ", RequestId::next()).is_none());
    assert!(append_block_to_last_user_message(&bytes, "", RequestId::next()).is_none());
    Ok(())
}
