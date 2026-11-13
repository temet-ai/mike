use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::steerer_repo::SteererPersist as _;
use tracing::{debug, info, warn};

use super::context::{EvalInputs, EvalTarget, FleetCtx, SteererSelection};
use super::steerer_eval::EvaluationScope;
use crate::steerer::inject::{ReplyDemand, build_steering_block_with};
use crate::steerer::registry::Steerer;
use crate::steerer::rollout_params::SteererClass;
use crate::steerer::verdict::SteererVerdict;
use crate::store::models::{ProxyEvent, ReminderSource, SystemReminder};
use crate::store::persistence::steering::{EnrolmentRow, NewIntervention};
use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Default)]
pub struct DeterministicOverrides {
    mailbox_pending: bool,
}

impl DeterministicOverrides {
    pub fn any(&self) -> bool {
        self.mailbox_pending
    }
}

async fn mailbox_pending_override(
    ctx: &FleetCtx,
    steerer: &Steerer,
    verdict_fires: bool,
    session_uuid: Option<uuid::Uuid>,
    target: &EvalTarget,
) -> bool {
    if steerer.kind != "verifier" || verdict_fires {
        return false;
    }
    match (ctx.persist.as_ref(), session_uuid) {
        (Some(ph), Some(uuid)) => match ph.launch().has_pending_inbound_mail(uuid).await {
            Ok(has_pending) => has_pending,
            Err(e) => {
                warn!(
                    request_id = target.raw_id,
                    session_id = %target.session_id,
                    steerer_id = steerer.id,
                    error = %e,
                    "steerer fleet: mailbox pending check failed — skipping override (fail-open to verdict)"
                );
                false
            }
        },
        _ => false,
    }
}

pub async fn deterministic_overrides(
    ctx: &FleetCtx,
    steerer: &Steerer,
    verdict: &SteererVerdict,
    session_uuid: Option<uuid::Uuid>,
    target: &EvalTarget,
) -> DeterministicOverrides {
    DeterministicOverrides {
        mailbox_pending: mailbox_pending_override(
            ctx,
            steerer,
            verdict.fires,
            session_uuid,
            target,
        )
        .await,
    }
}

pub fn select_nudge_message(
    overrides: &DeterministicOverrides,
    verdict: &SteererVerdict,
) -> String {
    if overrides.mailbox_pending {
        "You have an unread cross-session mailbox message (status: pending, unclaimed). \
         Check your mailbox (`GET /api/sessions/{id}/messages` or the mailbox tool) before \
         treating this session as done."
            .to_string()
    } else {
        verdict.nudge.clone()
    }
}

pub struct FireContext<'a> {
    pub ctx: &'a FleetCtx,
    pub steerer: &'a Steerer,
    pub effective_model: &'a str,
    pub selection: &'a SteererSelection<'a>,
    pub verdict: &'a SteererVerdict,
    pub target: &'a EvalTarget,
    pub eval_inputs: &'a EvalInputs,
    pub scope: Option<&'a EvaluationScope>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryRoute {
    Push,
    Append,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Addressing {
    pub route: DeliveryRoute,
    pub addressed_to: &'static str,
    pub actor_prefix: Option<String>,
}

pub fn addressing(steerer: &Steerer, scope: Option<&EvaluationScope>) -> Addressing {
    if let Some(actor) = scope.and_then(|s| s.actor.as_ref()) {
        let persona = actor.persona.as_deref().unwrap_or("agent");
        let agent = actor.agent_id.as_deref().unwrap_or("");
        return Addressing {
            route: DeliveryRoute::Append,
            addressed_to: "parent_for_actor",
            actor_prefix: Some(format!(
                "For the agent you dispatched ({persona} {agent}): this is about that agent's action, not yours."
            )),
        };
    }
    let route = match steerer.params.delivery.class {
        SteererClass::Efficiency => DeliveryRoute::Append,
        SteererClass::Correction => DeliveryRoute::Push,
    };
    Addressing {
        route,
        addressed_to: "session",
        actor_prefix: None,
    }
}

fn reply_demand(fire: &FireContext<'_>, authority: &str, addressing: &Addressing) -> ReplyDemand {
    if addressing.actor_prefix.is_some() {
        return ReplyDemand::None;
    }
    crate::steerer::inject::reply_demand_for_steerer(authority, fire.steerer.params.delivery.class)
}

fn note_text(addressing: &Addressing, nudge_message: &str) -> String {
    match &addressing.actor_prefix {
        Some(prefix) => format!("{prefix}\n\n{nudge_message}"),
        None => nudge_message.to_owned(),
    }
}

fn log_enrolled_firing(fire: &FireContext<'_>, enrolment: &EnrolmentRow) {
    debug!(
        request_id = fire.target.raw_id,
        session_id = %fire.target.session_id,
        steerer_id = fire.steerer.id,
        enrolment_id = enrolment.id,
        enrolled_by = %enrolment.enrolled_by,
        enrolled_at = %enrolment.enrolled_at,
        still_active = enrolment.disenrolled_at.is_none(),
        "steerer fleet: firing on enrolled session"
    );
}

fn log_enrolment_missing(fire: &FireContext<'_>) {
    warn!(
        request_id = fire.target.raw_id,
        session_id = %fire.target.session_id,
        steerer_id = fire.steerer.id,
        "steerer fleet: enrolled steerer missing from enrolment map — skipping"
    );
}

fn log_intervention_insert_failed(fire: &FireContext<'_>, e: &PersistError) {
    warn!(
        request_id = fire.target.raw_id,
        session_id = %fire.target.session_id,
        steerer_id = fire.steerer.id,
        error = %e,
        "steerer fleet: intervention insert failed — send aborted"
    );
}

async fn insert_enrolled_intervention(
    ph: &PersistHandle,
    fire: &FireContext<'_>,
    enrolment: &EnrolmentRow,
    verdict_code: &str,
    nudge_message: &str,
    intervention_uuid: uuid::Uuid,
) -> Option<u64> {
    match ph
        .steerer()
        .insert_intervention(NewIntervention {
            steerer_id: fire.steerer.id,
            enrolment_id: Some(enrolment.id),
            session_id: enrolment.session_id,
            verdict_code,
            mode_code: &enrolment.mode,
            authority_code: &enrolment.authority,
            trigger_request_id: fire.target.trigger_request_id,
            confidence: Some(fire.verdict.confidence),
            off_thread_topic: None,
            payload_text: Some(nudge_message),
            missing_evidence: None,
            model: Some(fire.effective_model),
            anchor_text: fire
                .eval_inputs
                .anchor
                .as_ref()
                .map(|a| a.state_of_play.as_str()),
            intervention_uuid,
            expires_at: chrono::Utc::now()
                + chrono::Duration::minutes(fire.ctx.config.payload_ttl_minutes),
            verdict_json: serde_json::to_value(fire.verdict).ok(),
            operator_turn_id: fire
                .eval_inputs
                .judged_turn_id
                .or(fire.scope.map(|s| s.operator_turn_id)),
            bundle_id: fire.ctx.settings.load().bundles.steerer(fire.steerer.id),
            delivery_channel: (addressing(fire.steerer, fire.scope).route == DeliveryRoute::Append)
                .then_some("append"),
            addressed_to: Some(addressing(fire.steerer, fire.scope).addressed_to),
            deliver_after: None,
            wake_state: None,
        })
        .await
    {
        Ok(id) => Some(id as u64),
        Err(e) => {
            log_intervention_insert_failed(fire, &e);
            None
        }
    }
}

async fn resolve_intervention_record(
    fire: &FireContext<'_>,
    verdict_code: &str,
    nudge_message: &str,
    intervention_uuid: uuid::Uuid,
) -> Option<(String, u64, String, String)> {
    let Some(enrolled_map) = fire.selection.enrolment_by_steerer_id.as_ref() else {
        return Some((
            intervention_uuid.to_string(),
            0u64,
            fire.steerer.authority.clone(),
            fire.steerer.mode.clone(),
        ));
    };

    let Some(enrolment) = enrolled_map.get(&fire.steerer.id) else {
        log_enrolment_missing(fire);
        return None;
    };

    log_enrolled_firing(fire, enrolment);

    let db_id: u64 = match fire.ctx.persist.as_ref() {
        Some(ph) => {
            insert_enrolled_intervention(
                ph,
                fire,
                enrolment,
                verdict_code,
                nudge_message,
                intervention_uuid,
            )
            .await?
        }
        None => 0,
    };

    Some((
        intervention_uuid.to_string(),
        db_id,
        enrolment.authority.clone(),
        enrolment.mode.clone(),
    ))
}

fn log_firing_intervention(fire: &FireContext<'_>) {
    info!(
        request_id = fire.target.raw_id,
        session_id = %fire.target.session_id,
        steerer_id = fire.steerer.id,
        steerer_name = %fire.steerer.name,
        kind = %fire.steerer.kind,
        confidence = fire.verdict.confidence,
        trigger_point = fire.eval_inputs.trigger_point,
        "steerer fleet: firing intervention"
    );
}

pub async fn record_and_broadcast(
    fire: &FireContext<'_>,
    nudge_message: &str,
) -> Option<(uuid::Uuid, SystemReminder)> {
    let intervention_uuid = uuid::Uuid::new_v4();

    let verdict_code: &str = "fired";
    let addressing = addressing(fire.steerer, fire.scope);
    let nudge_text = note_text(&addressing, nudge_message);
    let nudge_message = nudge_text.as_str();

    log_firing_intervention(fire);

    let (intervention_uuid_str, intervention_db_id, effective_authority, effective_mode) =
        resolve_intervention_record(fire, verdict_code, nudge_message, intervention_uuid).await?;

    let reminder = SystemReminder {
        source: ReminderSource::Steerer {
            kind: fire.steerer.kind.clone(),
            intervention_uuid: intervention_uuid_str,
            advisory: addressing.route == DeliveryRoute::Append
                && addressing.actor_prefix.is_none(),
        },
        payload: build_steering_block_with(
            nudge_message,
            reply_demand(fire, &effective_authority, &addressing),
        ),
    };
    deliver_fired_reminder(fire, &reminder, addressing.route).await;

    let _ = fire.ctx.event_tx.send(ProxyEvent::SteeringFired {
        intervention_id: intervention_db_id,
        session_id: fire.target.session_id.clone(),
        session_uuid: fire.target.session_id.clone(),
        steerer_name: Some(fire.steerer.name.clone()),
        steerer_display_name: Some(fire.steerer.display_name.clone()),
        authority: Some(effective_authority),
        mode: Some(effective_mode),
        verdict: Some(verdict_code.to_owned()),
        confidence: Some(fire.verdict.confidence as f32),
        off_thread_topic: Some(fire.verdict.reason.clone()),
        payload_text: Some(nudge_message.to_owned()),
        trigger_request_id: fire.target.raw_id,
    });

    Some((intervention_uuid, reminder))
}

async fn deliver_fired_reminder(
    fire: &FireContext<'_>,
    reminder: &SystemReminder,
    route: DeliveryRoute,
) {
    let recipient = match route {
        DeliveryRoute::Push => fire.target.session_id.parse::<uuid::Uuid>().ok(),
        DeliveryRoute::Append => None,
    };
    let Some(recipient) = recipient else {
        let mut store = fire.ctx.store.write().await;
        crate::injection::enqueue_if_admitted(
            &fire.ctx.injection_gate,
            &mut store,
            &fire.target.session_id,
            reminder.clone(),
        );
        return;
    };
    crate::proxy::remote_control::inject::deliver_or_enqueue(
        &fire.ctx.push_deps(),
        recipient,
        reminder.clone(),
    )
    .await;
}
