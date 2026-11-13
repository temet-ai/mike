use std::collections::HashMap;

use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

use super::transcript::{Direction, RECONNECT, Transcript, connection};
use super::{IngestConnection, IngestOutcome, IngestSource, Reconciler};
use crate::runtime::codex::RequestId;
use crate::runtime::codex::history::{HistoryBlocker, HistoryErrorKind, HistoryQuery};

type QueryKey = (String, Option<String>, Option<String>, Option<String>);

fn key_of(method: &str, params: &Value) -> QueryKey {
    let text = |key: &str| params.get(key).and_then(Value::as_str).map(str::to_owned);
    (
        method.to_owned(),
        text("threadId"),
        text("turnId"),
        text("cursor"),
    )
}

fn query_key(query: &HistoryQuery) -> QueryKey {
    (
        query.method().to_owned(),
        Some(query.identity().thread_id().to_owned()),
        query.turn_id().map(str::to_owned),
        query.cursor().map(str::to_owned),
    )
}

struct Recorded {
    requests: HashMap<QueryKey, RequestId>,
    responses: HashMap<RequestId, String>,
}

fn recorded(transcript: &Transcript) -> anyhow::Result<Recorded> {
    let mut recorded = Recorded {
        requests: HashMap::new(),
        responses: HashMap::new(),
    };
    for (direction, frame) in &transcript.frames {
        let value: Value = serde_json::from_str(frame)?;
        let Some(id) = value.get("id").cloned() else {
            continue;
        };
        let id: RequestId = serde_json::from_value(id)?;
        match (direction, value.get("method").and_then(Value::as_str)) {
            (Direction::Sent, Some(method)) => {
                let params = value.get("params").cloned().unwrap_or(Value::Null);
                recorded.requests.insert(key_of(method, &params), id);
            }
            (Direction::Received, None) => {
                recorded.responses.insert(id, frame.clone());
            }
            _ => {}
        }
    }
    Ok(recorded)
}

fn answer(
    reconciler: &mut Reconciler,
    connection: &mut IngestConnection,
    request_id: RequestId,
    response: &str,
) -> anyhow::Result<bool> {
    let Some(request) = reconciler.next_request(request_id) else {
        return Ok(false);
    };
    connection.register(request.rpc_scope());
    match connection.received(response.as_bytes().to_vec(), Utc::now())? {
        IngestOutcome::Capture(frame) => reconciler.on_response(&frame.observation, &request),
        IngestOutcome::Excluded { reason, .. } => anyhow::bail!("history excluded: {reason:?}"),
    }
    Ok(true)
}

#[test]
fn reconcile_reads_metadata_turns_and_every_turn_items_page() -> anyhow::Result<()> {
    let transcript = Transcript::parse(RECONNECT)?;
    let recorded = recorded(&transcript)?;
    let thread = transcript.header_text("thread_id")?;
    let mut connection = connection(&transcript)?;
    let mut reconciler = Reconciler::new(Uuid::from_u128(7), 100);
    reconciler.add_thread(&thread)?;
    reconciler.add_thread(&thread)?;
    let mut pages = 0;
    while !reconciler.finished() {
        let mut probe = reconciler.clone();
        let query = probe
            .next_query()
            .ok_or_else(|| anyhow::anyhow!("queue empty"))?;
        let id = recorded
            .requests
            .get(&query_key(&query))
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no recorded request for {:?}", query_key(&query)))?;
        let response = recorded
            .responses
            .get(&id)
            .ok_or_else(|| anyhow::anyhow!("no recorded response for {id:?}"))?;
        answer(&mut reconciler, &mut connection, id, response)?;
        pages += 1;
    }
    let report = reconciler.report();
    assert_eq!(pages, 5);
    assert_eq!(report.pages_read, 5);
    assert_eq!(report.turns.len(), 3);
    assert!(report.items.len() >= 10, "{}", report.items.len());
    assert!(report.blockers.is_empty(), "{:?}", report.blockers);
    assert!(report.failed_pages.is_empty(), "{:?}", report.failed_pages);
    assert_eq!(report.undetermined_ends, 0);
    assert!(!report.coverage.uncertain_metadata);
    assert_eq!(report.coverage.undiscovered_entries, 0);
    Ok(())
}

fn synthetic() -> (IngestConnection, Reconciler) {
    let source = IngestSource {
        codex_version: "synthetic".into(),
    };
    (
        IngestConnection::new(Uuid::from_u128(9), source),
        Reconciler::new(Uuid::from_u128(9), 2),
    )
}

fn response(id: &str, result: Value) -> String {
    json!({"id": id, "result": result}).to_string()
}

#[test]
fn reconcile_blocks_on_a_repeated_cursor() -> anyhow::Result<()> {
    let (mut connection, mut reconciler) = synthetic();
    reconciler.add_thread("t")?;
    let thread = json!({"thread": {"id": "t", "historyMode": "paginated", "ephemeral": false}});
    answer(
        &mut reconciler,
        &mut connection,
        RequestId::String("a".into()),
        &response("a", thread),
    )?;
    let page = json!({"data": [], "nextCursor": "c1"});
    answer(
        &mut reconciler,
        &mut connection,
        RequestId::String("b".into()),
        &response("b", page.clone()),
    )?;
    answer(
        &mut reconciler,
        &mut connection,
        RequestId::String("c".into()),
        &response("c", page),
    )?;
    assert!(reconciler.finished());
    assert_eq!(
        reconciler.report().blockers,
        vec![("t".to_owned(), HistoryBlocker::CursorCycle)]
    );
    Ok(())
}

#[test]
fn reconcile_records_rpc_error_and_undetermined_end() -> anyhow::Result<()> {
    let (mut connection, mut reconciler) = synthetic();
    reconciler.add_thread("t")?;
    reconciler.add_thread("u")?;
    let error = json!({"id": "a", "error": {"code": -32600, "message": "gone"}}).to_string();
    answer(
        &mut reconciler,
        &mut connection,
        RequestId::String("a".into()),
        &error,
    )?;
    let thread = json!({"thread": {"id": "u", "historyMode": "paginated", "ephemeral": false}});
    answer(
        &mut reconciler,
        &mut connection,
        RequestId::String("b".into()),
        &response("b", thread),
    )?;
    answer(
        &mut reconciler,
        &mut connection,
        RequestId::String("c".into()),
        &response("c", json!({"data": []})),
    )?;
    let report = reconciler.report();
    assert_eq!(
        report.failed_pages,
        vec![("t".to_owned(), HistoryErrorKind::RpcError)]
    );
    assert_eq!(report.undetermined_ends, 1);
    assert!(reconciler.finished());
    Ok(())
}
