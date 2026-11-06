use std::collections::BTreeMap;

use serde_json::{Value, json};
use uuid::Uuid;

use mike::runtime::codex::{
    CommandFacts, Evidence, EvidenceOrigin, Field, ItemCategory, ItemKey, ItemSnapshot,
    ProjectedItem, ReceiptId, SnapshotKind, TextChannel, ToolFacts,
};
use mike::store::persistence::PersistError;
use mike::store::persistence::runtime::codex::test_seams::{
    STATE_CODEC_VERSION, decode_item, encode_item,
};

fn evidence(ingestion_id: u128, origin: EvidenceOrigin) -> Evidence {
    Evidence {
        receipt: ReceiptId {
            namespace_id: Uuid::from_u128(1),
            ingestion_id: Uuid::from_u128(ingestion_id),
        },
        json_pointer: "/result/turns/0/items/1\0保留".into(),
        origin,
    }
}

fn snapshot() -> ItemSnapshot {
    ItemSnapshot {
        kind: SnapshotKind::Completed,
        provider_type: "unknown-provider-kind\0".into(),
        category: ItemCategory::Unknown,
        status: Field::Missing,
        text: Field::Value("snapshot\0text".into()),
        content: Field::Null,
        command: Some(CommandFacts {
            command: Field::Value("printf '\\0'".into()),
            cwd: Field::Missing,
            process_id: Field::Null,
            output: Field::Value("output\0".into()),
            exit_code: Field::Value(0),
            duration_ms: Field::Invalid(json!({"unexpected": null})),
            actions: Field::Value(vec![json!({"unknown": ["\0", null, 5]})]),
        }),
        tool: Some(ToolFacts {
            server: Field::Value(String::new()),
            namespace: Field::Null,
            tool: Field::Missing,
            arguments: Field::Value(json!({"opaque\0key": ["保留", "\0"]})),
            result: Field::Value(json!(null)),
            error: Field::Invalid(json!([true, "unknown"])),
            success: Field::Value(false),
        }),
        file_changes: Field::Invalid(json!({"future": {"field": "\0"}})),
        started_at_ms: Field::Value(i64::MIN),
        completed_at_ms: Field::Value(i64::MAX),
        provider_payload: json!({"unknown": {"nested": [null, "\0", {"n": 9}]}}),
    }
}

fn all_channels() -> BTreeMap<TextChannel, String> {
    BTreeMap::from([
        (TextChannel::Message, "message\0".into()),
        (TextChannel::Plan, String::new()),
        (TextChannel::CommandOutput, "stdout\0stderr".into()),
        (TextChannel::FileOutput, "保留\n".into()),
        (TextChannel::ReasoningContent(0), String::new()),
        (TextChannel::ReasoningContent(2), "content\0".into()),
        (TextChannel::ReasoningContent(i64::MAX), "last".into()),
        (TextChannel::ReasoningSummary(0), "summary\0".into()),
        (TextChannel::ReasoningSummary(2), String::new()),
        (TextChannel::ReasoningSummary(i64::MAX), "last\0".into()),
    ])
}

fn item() -> ProjectedItem {
    ProjectedItem {
        key: ItemKey {
            namespace_id: Uuid::from_u128(1),
            thread_id: "Thread:保留 ".into(),
            turn_id: "Turn:A".into(),
            item_id: "Item:1".into(),
        },
        snapshot: Some(snapshot()),
        display_text: all_channels(),
        streamed_text: BTreeMap::from([(TextChannel::Message, "different\0stream".into())]),
        terminal: true,
        reconciliation_required: true,
        snapshot_evidence: Some(evidence(2, EvidenceOrigin::RpcResponse)),
        first_evidence: evidence(3, EvidenceOrigin::LiveFrame),
        latest_evidence: evidence(4, EvidenceOrigin::RpcResponse),
    }
}

fn document(item: &ProjectedItem) -> anyhow::Result<Value> {
    Ok(serde_json::from_str(&encode_item(item)?)?)
}

fn assert_invalid(document: &Value, key: &ItemKey) -> anyhow::Result<()> {
    assert!(decode_item(&serde_json::to_string(document)?, key).is_err());
    Ok(())
}

#[test]
fn codec_all_channels_and_snapshot_fields_round_trip() -> anyhow::Result<()> {
    let mut original = item();
    original.streamed_text = all_channels();
    let encoded = encode_item(&original)?;
    assert!(encoded.contains("\\u0000"));
    assert!(!encoded.contains('\0'));
    assert_eq!(decode_item(&encoded, &original.key)?, original);
    let stored: Value = serde_json::from_str(&encoded)?;
    assert_eq!(stored["state_codec_version"], json!(STATE_CODEC_VERSION));
    assert_eq!(stored["display_text"].as_array().map(Vec::len), Some(10));
    assert_eq!(stored["streamed_text"].as_array().map(Vec::len), Some(10));
    Ok(())
}

#[test]
fn codec_absent_snapshot_and_empty_channels_round_trip() -> anyhow::Result<()> {
    let mut original = item();
    original.snapshot = None;
    original.snapshot_evidence = None;
    original.display_text.clear();
    original.streamed_text.clear();
    original.terminal = false;
    original.reconciliation_required = false;
    assert_eq!(
        decode_item(&encode_item(&original)?, &original.key)?,
        original
    );
    Ok(())
}

#[test]
fn codec_distinct_stream_and_display_round_trip() -> anyhow::Result<()> {
    let original = item();
    assert_eq!(
        decode_item(&encode_item(&original)?, &original.key)?,
        original
    );
    Ok(())
}

#[test]
fn codec_unsupported_versions_are_rejected() -> anyhow::Result<()> {
    let original = item();
    for version in [i32::MIN, -1, 0, 2, i32::MAX] {
        let mut stored = document(&original)?;
        stored["state_codec_version"] = json!(version);
        assert_invalid(&stored, &original.key)?;
    }
    Ok(())
}

#[test]
fn codec_mismatched_item_identity_is_rejected() -> anyhow::Result<()> {
    let original = item();
    let encoded = encode_item(&original)?;
    let mut mismatches = vec![original.key.clone(); 4];
    mismatches[0].namespace_id = Uuid::from_u128(99);
    mismatches[1].thread_id.push('x');
    mismatches[2].turn_id.push('x');
    mismatches[3].item_id.push('x');
    for key in mismatches {
        assert!(matches!(
            decode_item(&encoded, &key),
            Err(PersistError::DataInvariant(_))
        ));
    }
    Ok(())
}

#[test]
fn codec_duplicate_channels_in_either_collection_are_rejected() -> anyhow::Result<()> {
    let original = item();
    for collection in ["display_text", "streamed_text"] {
        for tag in [
            "message",
            "plan",
            "commandOutput",
            "fileOutput",
            "reasoningContent",
            "reasoningSummary",
        ] {
            let mut stored = document(&original)?;
            let index = if tag.starts_with("reasoning") {
                json!(3)
            } else {
                Value::Null
            };
            stored[collection] = json!([
                {"tag": tag, "index": index, "text": "first"},
                {"tag": tag, "index": index, "text": "second"}
            ]);
            assert_invalid(&stored, &original.key)?;
        }
    }
    Ok(())
}

#[test]
fn codec_nonindexed_channels_with_indices_are_rejected() -> anyhow::Result<()> {
    let original = item();
    for collection in ["display_text", "streamed_text"] {
        for tag in ["message", "plan", "commandOutput", "fileOutput"] {
            for index in [-1, 0, i64::MAX] {
                let mut stored = document(&original)?;
                stored[collection] = json!([{"tag": tag, "index": index, "text": ""}]);
                assert_invalid(&stored, &original.key)?;
            }
        }
    }
    Ok(())
}

#[test]
fn codec_reasoning_without_valid_indices_is_rejected() -> anyhow::Result<()> {
    let original = item();
    for collection in ["display_text", "streamed_text"] {
        for tag in ["reasoningContent", "reasoningSummary"] {
            for index in [
                json!(null),
                json!(-1),
                json!(i64::MIN),
                json!("0"),
                json!(0.5),
            ] {
                let mut stored = document(&original)?;
                stored[collection] = json!([{"tag": tag, "index": index, "text": ""}]);
                assert_invalid(&stored, &original.key)?;
            }
            let mut stored = document(&original)?;
            stored[collection] = json!([{"tag": tag, "text": ""}]);
            assert_invalid(&stored, &original.key)?;
        }
    }
    Ok(())
}

#[test]
fn encode_negative_reasoning_indices_is_rejected() {
    for streamed in [false, true] {
        for channel in [
            TextChannel::ReasoningContent(-1),
            TextChannel::ReasoningSummary(-1),
        ] {
            let mut original = item();
            let channels = if streamed {
                &mut original.streamed_text
            } else {
                &mut original.display_text
            };
            channels.insert(channel, String::new());
            assert!(matches!(
                encode_item(&original),
                Err(PersistError::DataInvariant(_))
            ));
        }
    }
}

#[test]
fn codec_invalid_evidence_receipts_are_rejected_everywhere() -> anyhow::Result<()> {
    let original = item();
    for location in ["first_evidence", "latest_evidence", "snapshot_evidence"] {
        for (field, id) in [
            ("namespace_id", Uuid::from_u128(99)),
            ("namespace_id", Uuid::nil()),
            ("ingestion_id", Uuid::nil()),
        ] {
            let mut stored = document(&original)?;
            stored[location]["receipt"][field] = json!(id);
            assert_invalid(&stored, &original.key)?;
        }
    }
    Ok(())
}

#[test]
fn encode_invalid_evidence_receipts_are_rejected_everywhere() -> anyhow::Result<()> {
    for location in 0..3 {
        for variant in 0..3 {
            let mut original = item();
            let evidence = match location {
                0 => &mut original.first_evidence,
                1 => &mut original.latest_evidence,
                _ => original
                    .snapshot_evidence
                    .as_mut()
                    .ok_or_else(|| anyhow::anyhow!("missing fixture evidence"))?,
            };
            match variant {
                0 => evidence.receipt.namespace_id = Uuid::from_u128(99),
                1 => evidence.receipt.namespace_id = Uuid::nil(),
                _ => evidence.receipt.ingestion_id = Uuid::nil(),
            }
            assert!(matches!(
                encode_item(&original),
                Err(PersistError::DataInvariant(_))
            ));
        }
    }
    Ok(())
}

#[test]
fn codec_nil_namespace_even_when_consistent_is_rejected() -> anyhow::Result<()> {
    let original = item();
    let mut stored = document(&original)?;
    let mut key = original.key;
    key.namespace_id = Uuid::nil();
    stored["key"] = json!(key);
    for location in ["first_evidence", "latest_evidence", "snapshot_evidence"] {
        stored[location]["receipt"]["namespace_id"] = json!(Uuid::nil());
    }
    assert_invalid(&stored, &key)
}

#[test]
fn codec_invalid_json_and_schema_are_rejected() -> anyhow::Result<()> {
    let original = item();
    for invalid in ["", "{", "null", "[]", "{}"] {
        assert!(decode_item(invalid, &original.key).is_err());
    }
    for (field, value) in [
        ("state_codec_version", json!(null)),
        ("state_codec_version", json!("1")),
        ("display_text", json!({"Message": "text"})),
        ("streamed_text", json!(null)),
        ("terminal", json!("false")),
        ("first_evidence", json!(null)),
        ("unexpected_field", json!(true)),
    ] {
        let mut stored = document(&original)?;
        stored[field] = value;
        assert_invalid(&stored, &original.key)?;
    }
    Ok(())
}

#[test]
fn codec_invalid_channel_schema_is_rejected() -> anyhow::Result<()> {
    let original = item();
    for channel in [
        json!({"tag": "unknown", "index": null, "text": ""}),
        json!({"tag": "Message", "index": null, "text": ""}),
        json!({"tag": "message", "index": null}),
        json!({"tag": "message", "index": null, "text": null}),
        json!({"tag": "message", "index": null, "text": 1}),
        json!({"tag": "message", "index": null, "text": "", "future": true}),
        json!({"tag": "message", "text": ""}),
        json!({"index": null, "text": ""}),
    ] {
        for collection in ["display_text", "streamed_text"] {
            let mut stored = document(&original)?;
            stored[collection] = json!([channel]);
            assert_invalid(&stored, &original.key)?;
        }
    }
    Ok(())
}

#[test]
fn codec_duplicate_document_fields_are_rejected() -> anyhow::Result<()> {
    let original = item();
    let encoded = encode_item(&original)?;
    let duplicate = encoded.replacen("{", "{\"state_codec_version\":1,", 1);
    assert!(decode_item(&duplicate, &original.key).is_err());
    let duplicate_channel = encoded.replacen(
        "\"tag\":\"message\"",
        "\"tag\":\"message\",\"tag\":\"plan\"",
        1,
    );
    assert!(decode_item(&duplicate_channel, &original.key).is_err());
    Ok(())
}

#[test]
fn codec_missing_document_fields_are_rejected() -> anyhow::Result<()> {
    let original = item();
    for field in [
        "state_codec_version",
        "key",
        "snapshot",
        "display_text",
        "streamed_text",
        "terminal",
        "reconciliation_required",
        "snapshot_evidence",
        "first_evidence",
        "latest_evidence",
    ] {
        let mut stored = document(&original)?;
        let object = stored
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("fixture state is not an object"))?;
        object.remove(field);
        assert_invalid(&stored, &original.key)?;
    }
    Ok(())
}
