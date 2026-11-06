use mike::runtime::codex::RequestId;
use mike::runtime::codex::history::{HistoryErrorKind, HistoryIdentity, HistoryQuery};
use serde_json::json;
use uuid::Uuid;

use super::history_support::{identity, items, turns};

#[test]
fn history_requests_use_metadata_and_explicit_ascending_pages() -> anyhow::Result<()> {
    let metadata = HistoryQuery::metadata(identity()?, 2)?.request(RequestId::String("007".into()));
    assert_eq!(
        metadata.wire_value(),
        json!({"id":"007", "method":"thread/read", "params":{
            "threadId":"Thread:保留 ", "includeTurns":false
        }})
    );
    assert_eq!(
        turns(None)?.wire_value(),
        json!({"id":7,"method":"thread/turns/list","params":{
            "threadId":"Thread:保留 ","sortDirection":"asc","itemsView":"notLoaded","limit":2,"cursor":null
        }})
    );
    assert_eq!(
        items(None)?.wire_value(),
        json!({"id":7,"method":"thread/items/list","params":{
            "threadId":"Thread:保留 ","turnId":"Turn:A","sortDirection":"asc","limit":2,"cursor":null
        }})
    );
    Ok(())
}

#[test]
fn history_requests_preserve_typed_id_and_opaque_cursor_bytes() -> anyhow::Result<()> {
    for cursor in ["", " aA/保留 \0\n ", "A", "a"] {
        let request = turns(Some(cursor))?;
        let bytes = serde_json::to_vec(&request.wire_value())?;
        let decoded: serde_json::Value = serde_json::from_slice(&bytes)?;
        assert_eq!(decoded["params"]["cursor"], cursor);
        assert_eq!(request.query().cursor(), Some(cursor));
        assert_eq!(request.rpc_scope().request_id, RequestId::Integer(7));
    }
    let request = items(None)?;
    let scope = request.rpc_scope();
    assert_eq!(scope.method, "thread/items/list");
    assert_eq!(scope.thread_id.as_deref(), Some("Thread:保留 "));
    assert_eq!(scope.turn_id.as_deref(), Some("Turn:A"));
    Ok(())
}

#[test]
fn history_requests_reject_invalid_identity_and_zero_limit() -> anyhow::Result<()> {
    let id = Uuid::from_u128(1);
    for (scan, namespace, thread) in [
        (Uuid::nil(), id, "thread"),
        (id, Uuid::nil(), "thread"),
        (id, id, " \t"),
        (id, id, "a\0b"),
    ] {
        assert_eq!(
            HistoryIdentity::new(scan, namespace, thread.into()).map_err(|e| e.kind),
            Err(HistoryErrorKind::InvalidIdentity)
        );
    }
    assert_eq!(
        HistoryQuery::metadata(identity()?, 0).map_err(|e| e.kind),
        Err(HistoryErrorKind::ZeroPageLimit)
    );
    assert_eq!(
        HistoryQuery::turns(identity()?, 0, None).map_err(|e| e.kind),
        Err(HistoryErrorKind::ZeroPageLimit)
    );
    assert_eq!(
        HistoryQuery::items(identity()?, "turn".into(), 0, None).map_err(|e| e.kind),
        Err(HistoryErrorKind::ZeroPageLimit)
    );
    for turn in ["", " \n", "a\0b"] {
        assert_eq!(
            HistoryQuery::items(identity()?, turn.into(), 2, None).map_err(|e| e.kind),
            Err(HistoryErrorKind::InvalidIdentity)
        );
    }
    Ok(())
}
