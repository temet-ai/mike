use super::EvalWindow;
use crate::steerer::inject::{ReplyDemand, build_steering_block_with};
use crate::steerer::registry::Steerer;
use crate::store::models::{ProxyEvent, ReminderSource, SystemReminder};
use crate::store::persistence::steerer_repo::SteererPersist as _;
use crate::store::persistence::steering::NewIntervention;

pub struct JevNote<'a> {
    pub topic: &'static str,
    pub text: &'a str,
    pub operator_turn_id: i64,
}

async fn insert(
    window: &EvalWindow<'_>,
    steerer: &Steerer,
    note: &JevNote<'_>,
    uuid: uuid::Uuid,
) -> Option<(i64, String, String)> {
    let persist = window.ctx.persist.as_ref()?;
    let enrolment = window
        .selection
        .enrolment_by_steerer_id
        .as_ref()
        .and_then(|m| m.get(&steerer.id))?;
    let id = persist
        .steerer()
        .insert_intervention(NewIntervention {
            steerer_id: steerer.id,
            enrolment_id: Some(enrolment.id),
            session_id: enrolment.session_id,
            verdict_code: "fired",
            mode_code: &enrolment.mode,
            authority_code: &enrolment.authority,
            trigger_request_id: window.target.trigger_request_id,
            confidence: None,
            off_thread_topic: Some(note.topic),
            payload_text: Some(note.text),
            missing_evidence: None,
            model: Some(window.base_llm.model()),
            anchor_text: None,
            intervention_uuid: uuid,
            expires_at: chrono::Utc::now()
                + chrono::Duration::minutes(window.ctx.config.payload_ttl_minutes),
            verdict_json: Some(serde_json::json!({ "jev_note": note.topic })),
            operator_turn_id: Some(note.operator_turn_id),
            bundle_id: None,
            delivery_channel: Some("append"),
            addressed_to: Some("session"),
            deliver_after: None,
            wake_state: None,
        })
        .await;
    match id {
        Ok(id) => Some((id, enrolment.authority.clone(), enrolment.mode.clone())),
        Err(error) => {
            tracing::warn!(topic = note.topic, %error, "jev note: intervention insert failed");
            None
        }
    }
}

pub async fn deliver(
    window: &EvalWindow<'_>,
    steerer: &Steerer,
    note: &JevNote<'_>,
) -> Option<uuid::Uuid> {
    let uuid = uuid::Uuid::new_v4();
    let (id, authority, mode) = insert(window, steerer, note, uuid).await?;
    let reminder = SystemReminder {
        source: ReminderSource::Steerer {
            kind: steerer.kind.clone(),
            intervention_uuid: uuid.to_string(),
            advisory: true,
        },
        payload: build_steering_block_with(note.text, ReplyDemand::None),
    };
    {
        let mut store = window.ctx.store.write().await;
        crate::injection::enqueue_if_admitted(
            &window.ctx.injection_gate,
            &mut store,
            &window.target.session_id,
            reminder,
        );
    }
    let _ = window.ctx.event_tx.send(ProxyEvent::SteeringFired {
        intervention_id: u64::try_from(id).unwrap_or(0),
        session_id: window.target.session_id.clone(),
        session_uuid: window.target.session_id.clone(),
        steerer_name: Some(steerer.name.clone()),
        steerer_display_name: Some(steerer.display_name.clone()),
        authority: Some(authority),
        mode: Some(mode),
        verdict: Some("fired".to_owned()),
        confidence: None,
        off_thread_topic: Some(note.topic.to_owned()),
        payload_text: Some(note.text.to_owned()),
        trigger_request_id: window.target.raw_id,
    });
    Some(uuid)
}
