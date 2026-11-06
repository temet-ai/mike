use super::*;
use serde_json::json;

fn drain_one(rx: &mut broadcast::Receiver<ProxyEvent>) -> Option<ProxyEvent> {
    rx.try_recv().ok()
}

#[test]
fn forward_valid_requests_insert_payload_emits_event() -> anyhow::Result<()> {
    let (tx, mut rx) = broadcast::channel(8);
    let payload = json!({
        "table": "requests",
        "id": "42",
        "session_id": "abcd",
        "op": "INSERT"
    })
    .to_string();
    forward_notification(&payload, &tx);
    let event = drain_one(&mut rx).ok_or_else(|| anyhow::anyhow!("no event broadcast"))?;
    match event {
        ProxyEvent::RowChanged {
            table,
            id,
            session_id,
            op,
        } => {
            assert_eq!(table, RowChangedTable::Requests);
            assert_eq!(id, "42");
            assert_eq!(session_id.as_deref(), Some("abcd"));
            assert_eq!(op, RowChangedOp::Insert);
        }
        other => anyhow::bail!("unexpected variant: {other:?}"),
    }
    Ok(())
}

#[test]
fn forward_sessions_update_payload_omits_session_id() -> anyhow::Result<()> {
    let (tx, mut rx) = broadcast::channel(8);
    let payload = json!({
        "table": "sessions",
        "id": "session-xyz",
        "op": "UPDATE"
    })
    .to_string();
    forward_notification(&payload, &tx);
    let event = drain_one(&mut rx).ok_or_else(|| anyhow::anyhow!("no event broadcast"))?;
    match event {
        ProxyEvent::RowChanged {
            table,
            id,
            session_id,
            op,
        } => {
            assert_eq!(table, RowChangedTable::Sessions);
            assert_eq!(id, "session-xyz");
            assert!(session_id.is_none());
            assert_eq!(op, RowChangedOp::Update);
        }
        other => anyhow::bail!("unexpected variant: {other:?}"),
    }
    Ok(())
}

#[test]
fn forward_filter_rules_delete_payload_emits_event() -> anyhow::Result<()> {
    let (tx, mut rx) = broadcast::channel(8);
    let uuid = "11111111-2222-3333-4444-555555555555";
    let payload = json!({
        "table": "filter_rules",
        "id": uuid,
        "op": "DELETE"
    })
    .to_string();
    forward_notification(&payload, &tx);
    let event = drain_one(&mut rx).ok_or_else(|| anyhow::anyhow!("no event broadcast"))?;
    match event {
        ProxyEvent::RowChanged {
            table,
            id,
            session_id,
            op,
        } => {
            assert_eq!(table, RowChangedTable::FilterRules);
            assert_eq!(id, uuid);
            assert!(session_id.is_none());
            assert_eq!(op, RowChangedOp::Delete);
        }
        other => anyhow::bail!("unexpected variant: {other:?}"),
    }
    Ok(())
}

#[test]
fn forward_prompt_compositions_insert_payload_emits_event() -> anyhow::Result<()> {
    let (tx, mut rx) = broadcast::channel(8);
    let uuid = "deadbeef-cafe-0000-0000-000000000001";
    let payload = json!({
        "table": "prompt_compositions",
        "id": uuid,
        "op": "INSERT"
    })
    .to_string();
    forward_notification(&payload, &tx);
    let event = drain_one(&mut rx).ok_or_else(|| anyhow::anyhow!("no event broadcast"))?;
    match event {
        ProxyEvent::RowChanged {
            table,
            id,
            session_id: _,
            op,
        } => {
            assert_eq!(table, RowChangedTable::PromptCompositions);
            assert_eq!(id, uuid);
            assert_eq!(op, RowChangedOp::Insert);
        }
        other => anyhow::bail!("unexpected variant: {other:?}"),
    }
    Ok(())
}

#[test]
fn forward_mailbox_messages_insert_payload_emits_event() -> anyhow::Result<()> {
    let (tx, mut rx) = broadcast::channel(8);
    let uuid = "deadbeef-cafe-0000-0000-000000000010";
    let payload = json!({
        "table": "mailbox_messages",
        "id": uuid,
        "session_id": "recipient-session",
        "op": "INSERT"
    })
    .to_string();
    forward_notification(&payload, &tx);
    let event = drain_one(&mut rx).ok_or_else(|| anyhow::anyhow!("no event broadcast"))?;
    match event {
        ProxyEvent::RowChanged {
            table,
            id,
            session_id,
            op,
        } => {
            assert_eq!(table, RowChangedTable::MailboxMessages);
            assert_eq!(id, uuid);
            assert_eq!(session_id.as_deref(), Some("recipient-session"));
            assert_eq!(op, RowChangedOp::Insert);
        }
        other => anyhow::bail!("unexpected variant: {other:?}"),
    }
    Ok(())
}

#[test]
fn forward_mailbox_threads_update_payload_emits_event() -> anyhow::Result<()> {
    let (tx, mut rx) = broadcast::channel(8);
    let uuid = "deadbeef-cafe-0000-0000-000000000011";
    let payload = json!({
        "table": "mailbox_threads",
        "id": uuid,
        "op": "UPDATE"
    })
    .to_string();
    forward_notification(&payload, &tx);
    let event = drain_one(&mut rx).ok_or_else(|| anyhow::anyhow!("no event broadcast"))?;
    match event {
        ProxyEvent::RowChanged {
            table,
            id,
            session_id,
            op,
        } => {
            assert_eq!(table, RowChangedTable::MailboxThreads);
            assert_eq!(id, uuid);
            assert!(session_id.is_none());
            assert_eq!(op, RowChangedOp::Update);
        }
        other => anyhow::bail!("unexpected variant: {other:?}"),
    }
    Ok(())
}

fn assert_resync_required(event: ProxyEvent, reason: ResyncReason) -> anyhow::Result<()> {
    match event {
        ProxyEvent::ResyncRequired { reason: r } => {
            assert_eq!(r, reason);
            Ok(())
        }
        other => anyhow::bail!("unexpected variant: {other:?}"),
    }
}

#[test]
fn forward_invalid_json_emits_unusable_notification_resync() -> anyhow::Result<()> {
    let (tx, mut rx) = broadcast::channel(8);
    forward_notification("not json", &tx);
    let event = drain_one(&mut rx).ok_or_else(|| anyhow::anyhow!("no event broadcast"))?;
    assert_resync_required(event, ResyncReason::UnusableNotification)
}

#[test]
fn forward_unknown_table_emits_unusable_notification_resync() -> anyhow::Result<()> {
    let (tx, mut rx) = broadcast::channel(8);
    let payload = json!({
        "table": "users",
        "id": "1",
        "op": "INSERT"
    })
    .to_string();
    forward_notification(&payload, &tx);
    let event = drain_one(&mut rx).ok_or_else(|| anyhow::anyhow!("no event broadcast"))?;
    assert_resync_required(event, ResyncReason::UnusableNotification)
}

#[test]
fn forward_unknown_op_emits_unusable_notification_resync() -> anyhow::Result<()> {
    let (tx, mut rx) = broadcast::channel(8);
    let payload = json!({
        "table": "requests",
        "id": "1",
        "op": "TRUNCATE"
    })
    .to_string();
    forward_notification(&payload, &tx);
    let event = drain_one(&mut rx).ok_or_else(|| anyhow::anyhow!("no event broadcast"))?;
    assert_resync_required(event, ResyncReason::UnusableNotification)
}

#[test]
fn forward_lesson_injections_update_payload_emits_event() -> anyhow::Result<()> {
    let (tx, mut rx) = broadcast::channel(8);
    let payload = json!({
        "table": "lesson_injections",
        "id": "7",
        "session_id": "sess-a",
        "op": "UPDATE"
    })
    .to_string();
    forward_notification(&payload, &tx);
    let event = drain_one(&mut rx).ok_or_else(|| anyhow::anyhow!("no event broadcast"))?;
    match event {
        ProxyEvent::RowChanged {
            table,
            id,
            session_id,
            op,
        } => {
            assert_eq!(table, RowChangedTable::LessonInjections);
            assert_eq!(id, "7");
            assert_eq!(session_id.as_deref(), Some("sess-a"));
            assert_eq!(op, RowChangedOp::Update);
        }
        other => anyhow::bail!("unexpected variant: {other:?}"),
    }
    Ok(())
}

#[test]
fn forward_steering_evaluations_insert_payload_emits_event() -> anyhow::Result<()> {
    let (tx, mut rx) = broadcast::channel(8);
    let uuid = "deadbeef-cafe-0000-0000-000000000020";
    let payload = json!({
        "table": "steering_evaluations",
        "id": uuid,
        "session_id": "sess-b",
        "op": "INSERT"
    })
    .to_string();
    forward_notification(&payload, &tx);
    let event = drain_one(&mut rx).ok_or_else(|| anyhow::anyhow!("no event broadcast"))?;
    match event {
        ProxyEvent::RowChanged {
            table,
            id,
            session_id,
            op,
        } => {
            assert_eq!(table, RowChangedTable::SteeringEvaluations);
            assert_eq!(id, uuid);
            assert_eq!(session_id.as_deref(), Some("sess-b"));
            assert_eq!(op, RowChangedOp::Insert);
        }
        other => anyhow::bail!("unexpected variant: {other:?}"),
    }
    Ok(())
}

#[test]
fn forward_consolidation_actions_insert_payload_emits_event() -> anyhow::Result<()> {
    let (tx, mut rx) = broadcast::channel(8);
    let payload = json!({
        "table": "consolidation_actions",
        "id": "99",
        "session_id": "sess-c",
        "op": "INSERT"
    })
    .to_string();
    forward_notification(&payload, &tx);
    let event = drain_one(&mut rx).ok_or_else(|| anyhow::anyhow!("no event broadcast"))?;
    match event {
        ProxyEvent::RowChanged {
            table,
            id,
            session_id,
            op,
        } => {
            assert_eq!(table, RowChangedTable::ConsolidationActions);
            assert_eq!(id, "99");
            assert_eq!(session_id.as_deref(), Some("sess-c"));
            assert_eq!(op, RowChangedOp::Insert);
        }
        other => anyhow::bail!("unexpected variant: {other:?}"),
    }
    Ok(())
}

#[test]
fn forward_consolidation_runs_insert_payload_omits_session_id() -> anyhow::Result<()> {
    let (tx, mut rx) = broadcast::channel(8);
    let payload = json!({
        "table": "consolidation_runs",
        "id": "5",
        "op": "INSERT"
    })
    .to_string();
    forward_notification(&payload, &tx);
    let event = drain_one(&mut rx).ok_or_else(|| anyhow::anyhow!("no event broadcast"))?;
    match event {
        ProxyEvent::RowChanged {
            table,
            id,
            session_id,
            op,
        } => {
            assert_eq!(table, RowChangedTable::ConsolidationRuns);
            assert_eq!(id, "5");
            assert!(session_id.is_none());
            assert_eq!(op, RowChangedOp::Insert);
        }
        other => anyhow::bail!("unexpected variant: {other:?}"),
    }
    Ok(())
}

#[test]
fn forward_session_judges_verdicts_delete_payload_emits_event() -> anyhow::Result<()> {
    let (tx, mut rx) = broadcast::channel(8);
    let payload = json!({
        "table": "session_judges_verdicts",
        "id": "12",
        "session_id": "sess-d",
        "op": "DELETE"
    })
    .to_string();
    forward_notification(&payload, &tx);
    let event = drain_one(&mut rx).ok_or_else(|| anyhow::anyhow!("no event broadcast"))?;
    match event {
        ProxyEvent::RowChanged {
            table,
            id,
            session_id,
            op,
        } => {
            assert_eq!(table, RowChangedTable::SessionJudgesVerdicts);
            assert_eq!(id, "12");
            assert_eq!(session_id.as_deref(), Some("sess-d"));
            assert_eq!(op, RowChangedOp::Delete);
        }
        other => anyhow::bail!("unexpected variant: {other:?}"),
    }
    Ok(())
}

#[test]
fn forward_llm_calls_insert_payload_with_null_session_emits_event() -> anyhow::Result<()> {
    let (tx, mut rx) = broadcast::channel(8);
    let payload = json!({
        "table": "llm_calls",
        "id": "8",
        "op": "INSERT"
    })
    .to_string();
    forward_notification(&payload, &tx);
    let event = drain_one(&mut rx).ok_or_else(|| anyhow::anyhow!("no event broadcast"))?;
    match event {
        ProxyEvent::RowChanged {
            table,
            id,
            session_id,
            op,
        } => {
            assert_eq!(table, RowChangedTable::LlmCalls);
            assert_eq!(id, "8");
            assert!(session_id.is_none());
            assert_eq!(op, RowChangedOp::Insert);
        }
        other => anyhow::bail!("unexpected variant: {other:?}"),
    }
    Ok(())
}

#[test]
fn forward_session_embeddings_update_payload_emits_event() -> anyhow::Result<()> {
    let (tx, mut rx) = broadcast::channel(8);
    let uuid = "deadbeef-cafe-0000-0000-000000000030";
    let payload = json!({
        "table": "session_embeddings",
        "id": uuid,
        "session_id": uuid,
        "op": "UPDATE"
    })
    .to_string();
    forward_notification(&payload, &tx);
    let event = drain_one(&mut rx).ok_or_else(|| anyhow::anyhow!("no event broadcast"))?;
    match event {
        ProxyEvent::RowChanged {
            table,
            id,
            session_id,
            op,
        } => {
            assert_eq!(table, RowChangedTable::SessionEmbeddings);
            assert_eq!(id, uuid);
            assert_eq!(session_id.as_deref(), Some(uuid));
            assert_eq!(op, RowChangedOp::Update);
        }
        other => anyhow::bail!("unexpected variant: {other:?}"),
    }
    Ok(())
}

#[test]
fn forward_with_no_subscribers_does_not_panic() {
    let (tx, _rx) = broadcast::channel::<ProxyEvent>(8);
    drop(_rx);
    let payload = json!({
        "table": "requests",
        "id": "1",
        "op": "INSERT"
    })
    .to_string();
    forward_notification(&payload, &tx);
}

mod run_tests;
