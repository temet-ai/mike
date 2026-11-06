use super::*;

#[test]
fn row_changed_lesson_injections_update_serializes_with_session_id() -> anyhow::Result<()> {
    let event = ProxyEvent::RowChanged {
        table: crate::store::models::RowChangedTable::LessonInjections,
        id: "7".to_string(),
        session_id: Some("sess-a".to_string()),
        op: crate::store::models::RowChangedOp::Update,
    };
    assert_eq!(
        event_to_json(&event)?,
        json!({
            "type": "RowChanged",
            "table": "lesson_injections",
            "id": "7",
            "session_id": "sess-a",
            "op": "UPDATE"
        })
    );
    Ok(())
}

#[test]
fn row_changed_steering_evaluations_insert_serializes_with_snake_case_table() -> anyhow::Result<()>
{
    let event = ProxyEvent::RowChanged {
        table: crate::store::models::RowChangedTable::SteeringEvaluations,
        id: "deadbeef-cafe-0000-0000-000000000020".to_string(),
        session_id: Some("sess-b".to_string()),
        op: crate::store::models::RowChangedOp::Insert,
    };
    assert_eq!(
        event_to_json(&event)?,
        json!({
            "type": "RowChanged",
            "table": "steering_evaluations",
            "id": "deadbeef-cafe-0000-0000-000000000020",
            "session_id": "sess-b",
            "op": "INSERT"
        })
    );
    Ok(())
}

#[test]
fn row_changed_consolidation_actions_insert_serializes_with_session_id() -> anyhow::Result<()> {
    let event = ProxyEvent::RowChanged {
        table: crate::store::models::RowChangedTable::ConsolidationActions,
        id: "99".to_string(),
        session_id: Some("sess-c".to_string()),
        op: crate::store::models::RowChangedOp::Insert,
    };
    assert_eq!(
        event_to_json(&event)?,
        json!({
            "type": "RowChanged",
            "table": "consolidation_actions",
            "id": "99",
            "session_id": "sess-c",
            "op": "INSERT"
        })
    );
    Ok(())
}

#[test]
fn row_changed_consolidation_runs_insert_serializes_without_session_id() -> anyhow::Result<()> {
    let event = ProxyEvent::RowChanged {
        table: crate::store::models::RowChangedTable::ConsolidationRuns,
        id: "5".to_string(),
        session_id: None,
        op: crate::store::models::RowChangedOp::Insert,
    };
    assert_eq!(
        event_to_json(&event)?,
        json!({
            "type": "RowChanged",
            "table": "consolidation_runs",
            "id": "5",
            "session_id": null,
            "op": "INSERT"
        })
    );
    Ok(())
}

#[test]
fn row_changed_session_judges_verdicts_delete_serializes_with_session_id() -> anyhow::Result<()> {
    let event = ProxyEvent::RowChanged {
        table: crate::store::models::RowChangedTable::SessionJudgesVerdicts,
        id: "12".to_string(),
        session_id: Some("sess-d".to_string()),
        op: crate::store::models::RowChangedOp::Delete,
    };
    assert_eq!(
        event_to_json(&event)?,
        json!({
            "type": "RowChanged",
            "table": "session_judges_verdicts",
            "id": "12",
            "session_id": "sess-d",
            "op": "DELETE"
        })
    );
    Ok(())
}

#[test]
fn row_changed_llm_calls_insert_serializes_with_null_session_id() -> anyhow::Result<()> {
    let event = ProxyEvent::RowChanged {
        table: crate::store::models::RowChangedTable::LlmCalls,
        id: "8".to_string(),
        session_id: None,
        op: crate::store::models::RowChangedOp::Insert,
    };
    assert_eq!(
        event_to_json(&event)?,
        json!({
            "type": "RowChanged",
            "table": "llm_calls",
            "id": "8",
            "session_id": null,
            "op": "INSERT"
        })
    );
    Ok(())
}

#[test]
fn row_changed_session_embeddings_update_serializes_with_session_id() -> anyhow::Result<()> {
    let uuid = "deadbeef-cafe-0000-0000-000000000030";
    let event = ProxyEvent::RowChanged {
        table: crate::store::models::RowChangedTable::SessionEmbeddings,
        id: uuid.to_string(),
        session_id: Some(uuid.to_string()),
        op: crate::store::models::RowChangedOp::Update,
    };
    assert_eq!(
        event_to_json(&event)?,
        json!({
            "type": "RowChanged",
            "table": "session_embeddings",
            "id": uuid,
            "session_id": uuid,
            "op": "UPDATE"
        })
    );
    Ok(())
}

#[test]
fn resync_required_listener_establishment_serializes_with_type_tag() -> anyhow::Result<()> {
    let event = ProxyEvent::ResyncRequired {
        reason: crate::store::models::ResyncReason::ListenerEstablishment,
    };
    assert_eq!(
        event_to_json(&event)?,
        json!({
            "type": "ResyncRequired",
            "reason": "listener_establishment"
        })
    );
    Ok(())
}

#[test]
fn resync_required_recovery_serializes_with_type_tag() -> anyhow::Result<()> {
    let event = ProxyEvent::ResyncRequired {
        reason: crate::store::models::ResyncReason::Recovery,
    };
    assert_eq!(
        event_to_json(&event)?,
        json!({
            "type": "ResyncRequired",
            "reason": "recovery"
        })
    );
    Ok(())
}

#[test]
fn resync_required_unusable_notification_serializes_with_type_tag() -> anyhow::Result<()> {
    let event = ProxyEvent::ResyncRequired {
        reason: crate::store::models::ResyncReason::UnusableNotification,
    };
    assert_eq!(
        event_to_json(&event)?,
        json!({
            "type": "ResyncRequired",
            "reason": "unusable_notification"
        })
    );
    Ok(())
}
