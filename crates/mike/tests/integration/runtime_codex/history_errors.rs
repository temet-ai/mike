use mike::runtime::codex::RequestId;
use mike::runtime::codex::history::{HistoryErrorKind, HistoryPage, HistoryQuery};
use serde_json::json;
use uuid::Uuid;

use super::history_support::{frame, identity, items, raw, turns};

#[test]
fn history_control_duplicates_reject_cursor_movement_from_original_bytes() -> anyhow::Result<()> {
    let request = turns(None)?;
    for bytes in [
        br#"{"id":7,"result":{"data":[],"data":[],"nextCursor":null}}"#.as_slice(),
        br#"{"id":7,"result":{"data":[],"nextCursor":"A","nextCursor":null}}"#,
        br#"{"id":7,"result":{"data":[],"next\u0043ursor":"A","nextCursor":null}}"#,
        br#"{"id":7,"result":{"data":[],"backwardsCursor":"A","backwardsCursor":null}}"#,
        br#"{"id":7,"id":7,"result":{"data":[],"nextCursor":null}}"#,
        br#"{"id":7,"result":{"data":[]},"result":{"data":[],"nextCursor":null}}"#,
        br#"{"id":7,"result":{"data":[],"threadId":"x","threadId":"y"}}"#,
    ] {
        let observation = raw(bytes)?;
        assert_eq!(
            HistoryPage::decode(&observation, &request)
                .err()
                .map(|e| e.kind),
            Some(HistoryErrorKind::DuplicateControlField)
        );
        assert_eq!(observation.raw_payload, bytes);
    }
    Ok(())
}

#[test]
fn history_metadata_duplicate_identity_mode_and_ephemeral_are_ambiguous() -> anyhow::Result<()> {
    let request = HistoryQuery::metadata(identity()?, 2)?.request(RequestId::Integer(7));
    for fields in [
        r#""id":"Thread:保留 ","id":"Thread:保留 ""#,
        r#""id":"Thread:保留 ","historyMode":"legacy","historyMode":"paginated""#,
        r#""id":"Thread:保留 ","ephemeral":false,"ephemeral":true"#,
    ] {
        let bytes = format!(r#"{{"id":7,"result":{{"thread":{{{fields}}}}}}}"#);
        let observation = raw(bytes.as_bytes())?;
        assert_eq!(
            HistoryPage::decode(&observation, &request)
                .err()
                .map(|e| e.kind),
            Some(HistoryErrorKind::DuplicateControlField)
        );
    }
    Ok(())
}

#[test]
fn history_response_rejects_wrong_typed_id_method_and_error_envelopes() -> anyhow::Result<()> {
    let request = turns(None)?;
    for (bytes, kind) in [
        (
            br#"{"id":"7","result":{"data":[]}}"#.as_slice(),
            HistoryErrorKind::UncorrelatedResponse,
        ),
        (
            br#"{"result":{"data":[]}}"#,
            HistoryErrorKind::UncorrelatedResponse,
        ),
        (
            br#"{"id":8,"result":{"data":[]}}"#,
            HistoryErrorKind::UncorrelatedResponse,
        ),
        (
            br#"{"id":7,"method":null,"result":{"data":[]}}"#,
            HistoryErrorKind::InvalidEnvelope,
        ),
        (
            br#"{"id":7,"method":"account/login/completed","result":{"data":[]}}"#,
            HistoryErrorKind::InvalidEnvelope,
        ),
        (
            br#"{"id":7,"result":{"data":[]},"error":{}}"#,
            HistoryErrorKind::InvalidEnvelope,
        ),
        (
            br#"{"id":7,"error":{"code":-1,"message":"retained"}}"#,
            HistoryErrorKind::RpcError,
        ),
        (
            br#"{"id":7,"result":{"data":null}}"#,
            HistoryErrorKind::InvalidData,
        ),
        (
            br#"{"id":7,"result":{"data":{}}}"#,
            HistoryErrorKind::InvalidData,
        ),
        (br#"{"id":7,"result":[]}"#, HistoryErrorKind::InvalidData),
        (br#"{"id":7,"result":{}}"#, HistoryErrorKind::InvalidData),
        (
            br#"{"id":7,"result":{"data":[]}} broken"#,
            HistoryErrorKind::InvalidEnvelope,
        ),
    ] {
        let observation = raw(bytes)?;
        assert_eq!(
            HistoryPage::decode(&observation, &request)
                .err()
                .map(|e| e.kind),
            Some(kind)
        );
    }
    Ok(())
}

#[test]
fn history_request_and_envelope_scope_conflicts_cannot_advance() -> anyhow::Result<()> {
    let request = items(None)?;
    for result in [
        json!({"threadId":"other","data":[]}),
        json!({"turnId":"other","data":[]}),
        json!({"threadId":null,"data":[]}),
    ] {
        let observation = frame(result)?;
        assert!(HistoryPage::decode(&observation, &request).is_err());
    }
    let original = frame(json!({"data":[],"nextCursor":null}))?;
    for index in 0..3 {
        let mut observation = original.clone();
        match index {
            0 => observation.envelope.namespace_id = Uuid::from_u128(99),
            1 => observation.envelope.provider_thread_id = Some("other".into()),
            _ => observation.envelope.provider_turn_id = Some("other".into()),
        }
        assert_eq!(
            HistoryPage::decode(&observation, &request)
                .err()
                .map(|e| e.kind),
            Some(HistoryErrorKind::ConflictingScope)
        );
    }
    let mut inconsistent = original;
    inconsistent.parsed_payload = Some(json!({"id":7,"result":{"data":[],"nextCursor":"edited"}}));
    assert_eq!(
        HistoryPage::decode(&inconsistent, &request)
            .err()
            .map(|e| e.kind),
        Some(HistoryErrorKind::InvalidObservation)
    );
    Ok(())
}

#[test]
fn history_metadata_missing_wrong_or_invalid_thread_is_rejected() -> anyhow::Result<()> {
    let request = HistoryQuery::metadata(identity()?, 2)?.request(RequestId::Integer(7));
    for result in [
        json!({}),
        json!({"thread":null}),
        json!({"thread":{"id":"other"}}),
        json!({"thread":{"id":"a\u{0}b"}}),
    ] {
        let observation = frame(result)?;
        assert!(HistoryPage::decode(&observation, &request).is_err());
    }
    Ok(())
}

#[test]
fn history_duplicate_turn_and_entry_scope_keep_other_discoveries() -> anyhow::Result<()> {
    for (request, bytes) in [
        (turns(None)?, br#"{"id":7,"result":{"data":[{"id":"a","id":"b"},{"id":"c"}],"nextCursor":null}}"#.as_slice()),
        (items(None)?, br#"{"id":7,"result":{"data":[{"turnId":"other","turnId":"Turn:A","item":{"id":"a","type":"agentMessage"}},{"turnId":"Turn:A","item":{"id":"b","type":"agentMessage"}}],"nextCursor":null}}"#),
        (items(None)?, br#"{"id":7,"result":{"data":[{"turnId":"Turn:A","item":{"id":"a","type":"other","type":"agentMessage"}},{"turnId":"Turn:A","item":{"id":"b","type":"agentMessage"}}],"nextCursor":null}}"#),
    ] {
        let observation = raw(bytes)?;
        let page = HistoryPage::decode(&observation, &request)?;
        let entries = page.entries().collect::<Vec<_>>();
        assert_eq!(entries[0].as_ref().err().map(|e| e.kind), Some(HistoryErrorKind::DuplicateControlField));
        assert!(entries[1].is_ok());
    }
    Ok(())
}
