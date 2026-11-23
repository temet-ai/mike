use serde_json::json;

use super::*;
use crate::store::persistence::sent_content::seam::sent_round_trip_in_memory;

fn assert_round_trips(original: &JsonValue, sent: &JsonValue) -> anyhow::Result<()> {
    let rebuilt =
        sent_round_trip_in_memory(original.to_string().as_bytes(), sent.to_string().as_bytes())?;
    let rebuilt: JsonValue = serde_json::from_slice(&rebuilt)?;
    assert_eq!(&rebuilt, sent);
    Ok(())
}

fn conversation() -> JsonValue {
    json!({
        "model": "m",
        "system": [{"type": "text", "text": "S"}],
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "a"}]},
            {"role": "assistant", "content": "b"},
            {"role": "user", "content": [{"type": "text", "text": "c", "cache_control": {"type": "ephemeral"}}]}
        ],
        "tools": [{"name": "Read"}, {"name": "Bash"}],
        "stream": true
    })
}

fn edited(edit: impl FnOnce(&mut JsonValue)) -> JsonValue {
    let mut body = conversation();
    edit(&mut body);
    body
}

#[test]
fn assemble_sent_without_edits_is_the_original() -> anyhow::Result<()> {
    assert_round_trips(&conversation(), &conversation())
}

#[test]
fn assemble_sent_system_edit_round_trips() -> anyhow::Result<()> {
    assert_round_trips(
        &conversation(),
        &edited(|b| b["system"][0]["text"] = json!("S plus composer")),
    )
}

#[test]
fn assemble_sent_carried_message_edit_round_trips() -> anyhow::Result<()> {
    assert_round_trips(
        &conversation(),
        &edited(|b| b["messages"][0]["content"][0]["text"] = json!("[duplicate read removed]")),
    )
}

#[test]
fn assemble_sent_moved_cache_control_round_trips() -> anyhow::Result<()> {
    assert_round_trips(
        &conversation(),
        &edited(|b| {
            b["messages"][2]["content"][0] = json!({"type": "text", "text": "c"});
            b["messages"][0]["content"][0]["cache_control"] = json!({"type": "ephemeral"});
        }),
    )
}

#[test]
fn assemble_sent_message_count_change_round_trips() -> anyhow::Result<()> {
    assert_round_trips(
        &conversation(),
        &edited(|b| {
            if let Some(messages) = b["messages"].as_array_mut() {
                messages.insert(1, json!({"role": "user", "content": "mail"}));
            }
        }),
    )
}

#[test]
fn assemble_sent_edited_tools_round_trip() -> anyhow::Result<()> {
    assert_round_trips(
        &conversation(),
        &edited(|b| b["tools"] = json!([{"name": "Read"}, {"name": "Bash"}, {"name": "mike"}])),
    )
}

#[test]
fn assemble_sent_emptied_tools_round_trip() -> anyhow::Result<()> {
    assert_round_trips(&conversation(), &edited(|b| b["tools"] = json!([])))
}

#[test]
fn assemble_sent_dropped_messages_key_round_trips() -> anyhow::Result<()> {
    assert_round_trips(
        &conversation(),
        &edited(|b| {
            if let Some(map) = b.as_object_mut() {
                map.remove("messages");
            }
        }),
    )
}

#[test]
fn assemble_sent_count_without_every_ord_is_an_error() {
    let request = StoredRequest {
        id: 7,
        request_envelope: Some(br#"{"messages":[]}"#.to_vec()),
        sent_message_count: Some(2),
        ..StoredRequest::default()
    };
    let sent = [(1, json!({"role": "user", "content": "x"}))];
    let parts = RequestParts {
        sent_messages: &sent,
        ..RequestParts::default()
    };
    assert!(matches!(
        assemble_sent(&request, parts, None),
        Err(PersistError::DataInvariant(_))
    ));
}

#[test]
fn assemble_sent_override_past_the_original_is_an_error() {
    let request = StoredRequest {
        id: 7,
        sent_envelope: Some(br#"{"messages":[]}"#.to_vec()),
        ..StoredRequest::default()
    };
    let sent = [(3, json!({"role": "user", "content": "x"}))];
    let parts = RequestParts {
        sent_messages: &sent,
        ..RequestParts::default()
    };
    assert!(matches!(
        assemble_sent(&request, parts, None),
        Err(PersistError::DataInvariant(_))
    ));
}

#[test]
fn assemble_sent_legacy_patches_apply_to_the_raw_body() -> anyhow::Result<()> {
    let raw = br#"{"system":"S","messages":[]}"#;
    let request = StoredRequest {
        id: 7,
        request_envelope: Some(raw.to_vec()),
        envelope_patch: Some(SentPatchRow {
            sent_patch_start: 11,
            sent_patch_end: 12,
            sent_patch_bytes: b"S2".to_vec(),
        }),
        ..StoredRequest::default()
    };
    let parts = RequestParts::default();
    assert!(request.needs_raw_body(parts));
    let rebuilt = assemble_sent(&request, parts, Some(raw))?;
    assert_eq!(
        rebuilt.as_deref(),
        Some(br#"{"system":"S2","messages":[]}"#.as_slice())
    );
    Ok(())
}

#[test]
fn assemble_sent_content_without_any_envelope_serves_the_raw_body() -> anyhow::Result<()> {
    let request = StoredRequest::default();
    let parts = RequestParts::default();
    assert!(request.needs_raw_body(parts));
    assert_eq!(
        assemble_sent(&request, parts, Some(b"{}"))?.as_deref(),
        Some(b"{}".as_slice())
    );
    assert_eq!(assemble_sent(&request, parts, None)?, None);
    Ok(())
}

#[test]
fn same_json_ignores_key_order_and_rejects_non_json() {
    assert!(same_json(br#"{"a":1,"b":[2]}"#, br#"{ "b": [2], "a": 1 }"#));
    assert!(!same_json(br#"{"a":1}"#, br#"{"a":2}"#));
    assert!(!same_json(b"not json", b"not json"));
}
