use serde_json::{Value, json};

use super::super::samples::*;
use super::*;

fn raw(value: &Value) -> anyhow::Result<Vec<u8>> {
    Ok(serde_json::to_vec(value)?)
}

fn scanned(body: &Value, seen_before: Seen) -> anyhow::Result<Scanned> {
    scan_body(&raw(body)?, seen_before)
        .ok_or_else(|| anyhow::anyhow!("a body that holds a notice must be scanned"))
}

fn thinned_value(body: &Value) -> anyhow::Result<(Value, Removed)> {
    let scanned = scanned(body, Seen::default())?;
    let bytes = scanned
        .thinned
        .ok_or_else(|| anyhow::anyhow!("expected the body to be thinned"))?;
    Ok((serde_json::from_slice(&bytes)?, scanned.removed))
}

#[test]
fn a_body_without_a_notice_is_not_scanned() -> anyhow::Result<()> {
    let body = json!({"messages": [{"role": "user", "content": "hello"}]});
    assert!(scan_body(&raw(&body)?, [true, true, true]).is_none());
    assert!(
        scan_body(
            b"IMPORTANT: This is NOT from your user, not json",
            [true, true, true]
        )
        .is_none()
    );
    Ok(())
}

#[test]
fn a_later_message_sent_mid_turn_is_forwarded_as_the_operators_words() -> anyhow::Result<()> {
    let body = json!({"messages": [
        {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "t1", "content": "rc=0"},
            {"type": "text", "text": sent_mid_turn("first")}
        ]},
        {"role": "assistant", "content": [{"type": "text", "text": "ok"}]},
        {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "t2", "content": "rc=0"},
            {"type": "text", "text": sent_mid_turn("second")}
        ]}
    ]});
    let (value, removed) = thinned_value(&body)?;
    assert_eq!(removed, [0, 0, 1]);
    assert_eq!(value["messages"][0], body["messages"][0]);
    assert_eq!(
        value["messages"][2]["content"][0],
        body["messages"][2]["content"][0]
    );
    assert_eq!(
        value["messages"][2]["content"][1]["text"],
        "The user sent a new message while you were working:\nsecond"
    );
    Ok(())
}

#[test]
fn a_body_with_one_notice_keeps_it_and_reports_it_seen() -> anyhow::Result<()> {
    let body = json!({"messages": [
        {"role": "user", "content": pushed_while_idle("only")}
    ]});
    let scanned = scanned(&body, Seen::default())?;
    assert!(scanned.thinned.is_none());
    assert_eq!(scanned.seen, [true, false, false]);
    Ok(())
}

#[test]
fn the_first_pushed_note_keeps_the_notice_and_later_ones_lose_it() -> anyhow::Result<()> {
    let body = json!({"messages": [
        {"role": "user", "content": pushed_while_idle("one")},
        {"role": "assistant", "content": [{"type": "text", "text": "ok"}]},
        {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "t1", "content": "rc=0"},
            {"type": "text", "text": pushed_while_working("two")}
        ]},
        {"role": "assistant", "content": [{"type": "text", "text": "ok"}]},
        {"role": "user", "content": pushed_while_idle("three")}
    ]});
    let (value, removed) = thinned_value(&body)?;
    assert_eq!(removed, [2, 0, 0]);
    assert_eq!(value["messages"][0], body["messages"][0]);
    let second = value["messages"][2]["content"][1]["text"]
        .as_str()
        .unwrap_or_default();
    assert!(second.contains("two") && !second.contains("permission laundering"));
    let third = value["messages"][4]["content"].as_str().unwrap_or_default();
    assert!(third.ends_with("Act on it, or say briefly why it doesn't apply.</system-reminder>"));
    Ok(())
}

#[test]
fn mail_in_system_messages_keeps_the_first_channel_notice_only() -> anyhow::Result<()> {
    let body = json!({"messages": [
        {"role": "user", "content": "start"},
        {"role": "system", "content": mail_while_working("one")},
        {"role": "system", "content": [{"type": "text", "text": mail_while_working("two")}]}
    ]});
    let (value, removed) = thinned_value(&body)?;
    assert_eq!(removed, [0, 1, 0]);
    assert_eq!(value["messages"][1], body["messages"][1]);
    assert_eq!(
        value["messages"][2]["content"][0]["text"],
        json!(
            "A message arrived from mike while you were working:\n<channel source=\"mike\" kind=\"mailbox_message\" message_id=\"e97fbe37\">\ntwo\n</channel>"
        )
    );
    Ok(())
}

#[test]
fn mail_appended_to_tool_results_is_thinned_in_both_content_shapes() -> anyhow::Result<()> {
    let body = json!({"messages": [
        {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "t1",
             "content": tool_output_with_mail("ok", &["one", "two"])}
        ]},
        {"role": "assistant", "content": [{"type": "text", "text": "ok"}]},
        {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "t2", "content": [
                {"type": "text", "text": tool_output_with_mail("ok", &["three"])}
            ]}
        ]}
    ]});
    let (value, removed) = thinned_value(&body)?;
    assert_eq!(removed, [0, 2, 0]);
    let first = value["messages"][0]["content"][0]["content"]
        .as_str()
        .unwrap_or_default();
    assert_eq!(first.matches("NOT from your user").count(), 1);
    assert!(first.contains("one") && first.contains("two"));
    let later = value["messages"][2]["content"][0]["content"][0]["text"]
        .as_str()
        .unwrap_or_default();
    assert!(later.contains("three") && !later.contains("NOT from your user"));
    Ok(())
}

#[test]
fn a_request_that_continues_a_thread_starts_from_what_the_session_has_seen() -> anyhow::Result<()> {
    let messages = json!([{"role": "user", "content": [
        {"type": "tool_result", "tool_use_id": "t1",
         "content": tool_output_with_mail("ok", &["later"])}
    ]}]);
    let continuing = json!({
        "messages": messages,
        "thread": {"type": "continue", "previous_message_id": "msg_1"}
    });
    let after_one = scanned(&continuing, [false, true, false])?;
    assert_eq!(after_one.removed, [0, 1, 0]);
    assert!(after_one.thinned.is_some());
    assert!(continues_a_thread(&raw(&continuing)?));

    let first_ever = scanned(&continuing, Seen::default())?;
    assert!(first_ever.thinned.is_none());
    assert_eq!(first_ever.seen, [false, true, false]);

    let whole = json!({"messages": messages, "thread": {"type": "create"}});
    let fresh = scanned(&whole, [true, true, true])?;
    assert!(
        fresh.thinned.is_none(),
        "a request that carries the whole conversation is judged by what it holds"
    );
    assert_eq!(fresh.seen, [false, true, false]);
    assert!(!continues_a_thread(&raw(&whole)?));
    Ok(())
}

#[test]
fn an_assistant_turn_and_printed_tool_output_that_quote_the_notice_are_not_touched()
-> anyhow::Result<()> {
    let quoted = format!("{}\n{}", pushed_while_idle("a"), pushed_while_idle("b"));
    let printed = format!("{}\n{}\nrows: 2", mail_block("x"), mail_block("y"));
    let body = json!({"messages": [
        {"role": "user", "content": pushed_while_idle("one")},
        {"role": "assistant", "content": [
            {"type": "text", "text": quoted},
            {"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": quoted}}
        ]},
        {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "t1", "content": quoted},
            {"type": "tool_result", "tool_use_id": "t2", "content": printed}
        ]}
    ]});
    let scanned = scanned(&body, Seen::default())?;
    assert!(scanned.thinned.is_none());
    assert_eq!(scanned.seen, [true, false, false]);
    Ok(())
}

#[test]
fn every_byte_outside_a_thinned_text_is_kept_as_sent() -> anyhow::Result<()> {
    let first = serde_json::to_string(&pushed_while_idle("one"))?;
    let second = serde_json::to_string(&pushed_while_idle("two"))?;
    let head = format!(
        "{{ \"model\":\"m\",\"messages\":[{{\"role\":\"user\",\"content\":{first}}},  {{\"content\":"
    );
    let tail = ",\"role\":\"user\"}],\"zeta\":1,\"alpha\":{\"b\":2,\"a\":1}}";
    let body = format!("{head}{second}{tail}");
    let bytes = scan_body(body.as_bytes(), Seen::default())
        .and_then(|scanned| scanned.thinned)
        .ok_or_else(|| anyhow::anyhow!("expected the body to be thinned"))?;
    let out = String::from_utf8(bytes)?;
    assert!(out.starts_with(&head));
    assert!(out.ends_with(tail));
    assert_eq!(out.matches("permission laundering").count(), 1);
    Ok(())
}

#[test]
fn thinning_the_same_history_twice_gives_the_same_bytes() -> anyhow::Result<()> {
    let mut messages = vec![
        json!({"role": "user", "content": pushed_while_idle("one")}),
        json!({"role": "user", "content": pushed_while_idle("two")}),
    ];
    let earlier = scan_body(&raw(&json!({"messages": messages}))?, Seen::default())
        .and_then(|scanned| scanned.thinned)
        .ok_or_else(|| anyhow::anyhow!("expected the body to be thinned"))?;
    messages.push(json!({"role": "user", "content": pushed_while_idle("three")}));
    let later = scan_body(&raw(&json!({"messages": messages}))?, Seen::default())
        .and_then(|scanned| scanned.thinned)
        .ok_or_else(|| anyhow::anyhow!("expected the body to be thinned"))?;
    let shared = earlier.len() - 2;
    assert_eq!(earlier[..shared], later[..shared]);
    Ok(())
}

#[test]
fn a_request_captured_from_claude_code_keeps_one_notice_for_three_mails() -> anyhow::Result<()> {
    let captured = include_str!("fixtures/mail_in_tool_result_claude_code_2_1_289.json");
    assert_eq!(captured.matches("NOT from your user").count(), 3);
    let scanned = scan_body(captured.as_bytes(), Seen::default())
        .ok_or_else(|| anyhow::anyhow!("the captured request holds notices"))?;
    assert_eq!(scanned.removed, [0, 2, 0]);
    assert_eq!(scanned.seen, [false, true, false]);
    let sent: Value = serde_json::from_slice(&scanned.thinned.unwrap_or_default())?;
    let content = sent["messages"][0]["content"][0]["content"]
        .as_str()
        .unwrap_or_default();
    assert_eq!(content.matches("NOT from your user").count(), 1);
    for note in [
        "note one of three",
        "note two of three",
        "note three of three",
    ] {
        assert!(content.contains(note), "{note} must still be delivered");
    }
    assert!(content.starts_with("ok\n\n<system-reminder>\nA message arrived from mike"));
    assert!(content.ends_with("tokens left</total_tokens>\n</system-reminder>"));

    let again = scan_body(captured.as_bytes(), scanned.seen)
        .ok_or_else(|| anyhow::anyhow!("the captured request holds notices"))?;
    assert_eq!(
        again.removed,
        [0, 3, 0],
        "a session that already had the notice gets none of the three"
    );
    Ok(())
}

#[test]
fn a_pushed_note_captured_from_claude_code_keeps_the_notice_once_per_session() -> anyhow::Result<()>
{
    let captured = include_str!("fixtures/pushed_note_claude_code_2_1_289.json");
    let first = scan_body(captured.as_bytes(), Seen::default())
        .ok_or_else(|| anyhow::anyhow!("the captured request holds a notice"))?;
    assert!(
        first.thinned.is_none(),
        "the first pushed note is forwarded whole"
    );
    assert_eq!(first.seen, [true, false, false]);

    let later = scan_body(captured.as_bytes(), first.seen)
        .ok_or_else(|| anyhow::anyhow!("the captured request holds a notice"))?;
    assert_eq!(later.removed, [1, 0, 0]);
    let sent: Value = serde_json::from_slice(&later.thinned.unwrap_or_default())?;
    assert_eq!(
        sent["messages"][0]["content"][0]["text"],
        json!(
            "Another Claude session sent a message:\n<system-reminder>Steering note from the operator's proxy:\n\nExample note: show the test output before reporting the work as done.\n\nAct on it, or say briefly why it doesn't apply.</system-reminder>"
        )
    );
    Ok(())
}
