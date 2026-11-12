use super::EvalWindow;
use crate::steerer::guard::live::LiveFire;
use crate::steerer::guard::live_decision::{DeliveryPlan, LiveDecision};
use crate::steerer::inject::build_steering_block_with;
use crate::steerer::registry::Steerer;
use crate::store::models::{ProxyEvent, ReminderSource, SystemReminder};
use crate::store::persistence::steering::NewIntervention;

fn plan_of(fire: &LiveFire) -> Option<DeliveryPlan> {
    match fire.decision {
        LiveDecision::Deliver { plan, .. } => Some(plan),
        _ => None,
    }
}

async fn insert(
    window: &EvalWindow<'_>,
    steerer: &Steerer,
    fire: &LiveFire,
    plan: DeliveryPlan,
    uuid: uuid::Uuid,
) -> Option<(i64, String, String)> {
    let persist = window.ctx.persist.as_ref()?;
    let enrolment = window
        .selection
        .enrolment_by_steerer_id
        .as_ref()
        .and_then(|m| m.get(&steerer.id))?;
    let scope = window.scope()?;
    let verdict_json = serde_json::json!({"question": fire.question, "decision": fire.decision});
    let id = persist
        .insert_intervention(NewIntervention {
            steerer_id: steerer.id,
            enrolment_id: Some(enrolment.id),
            session_id: enrolment.session_id,
            verdict_code: "fired",
            mode_code: &enrolment.mode,
            authority_code: &enrolment.authority,
            trigger_request_id: window.target.trigger_request_id,
            confidence: None,
            off_thread_topic: Some(fire.question.code()),
            payload_text: Some(&fire.nudge),
            missing_evidence: None,
            model: Some(window.base_llm.model()),
            anchor_text: None,
            intervention_uuid: uuid,
            expires_at: chrono::Utc::now()
                + chrono::Duration::minutes(window.ctx.config.payload_ttl_minutes),
            verdict_json: Some(verdict_json),
            operator_turn_id: Some(scope.operator_turn_id),
            bundle_id: window.ctx.settings.load().bundles.steerer(steerer.id),
            delivery_channel: plan.channel(),
            addressed_to: Some("session"),
            deliver_after: plan.deliver_after(),
            wake_state: plan.wake_state(),
        })
        .await;
    match id {
        Ok(id) => Some((id, enrolment.authority.clone(), enrolment.mode.clone())),
        Err(e) => {
            tracing::warn!(steerer_id = steerer.id, error = %e, "turn guard: intervention insert failed");
            None
        }
    }
}

pub(super) async fn deliver_guard_fire(
    window: &EvalWindow<'_>,
    steerer: &Steerer,
    fire: &LiveFire,
) -> Option<uuid::Uuid> {
    let plan = plan_of(fire)?;
    let uuid = uuid::Uuid::new_v4();
    let (id, authority, mode) = insert(window, steerer, fire, plan, uuid).await?;
    let reminder = SystemReminder {
        source: ReminderSource::Steerer {
            kind: steerer.kind.clone(),
            intervention_uuid: uuid.to_string(),
            advisory: false,
        },
        payload: build_steering_block_with(
            &fire.nudge,
            crate::steerer::inject::reply_demand_for_steerer(
                &authority,
                steerer.params.delivery.class,
            ),
        ),
    };
    if plan.enqueues() {
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
        off_thread_topic: Some(fire.question.code().to_owned()),
        payload_text: Some(fire.nudge.clone()),
        trigger_request_id: window.target.raw_id,
    });
    Some(uuid)
}
