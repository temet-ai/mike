use crate::store::persistence::steerer_repo::SteererPersist as _;
use tracing::{debug, warn};

use super::judgement::TurnJudgement;
use crate::steerer::fleet::context::{EvalInputs, EvalTarget, FleetCtx};
use crate::steerer::registry::Steerer;
use crate::steerer::rollout_params::Rollout;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::operator_turns::TriggerTurn;
use crate::store::persistence::steering_evaluations::{EvaluationMode, NewEvaluation};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActorRef {
    pub persona: Option<String>,
    pub agent_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluationScope {
    pub session_db_id: i64,
    pub operator_turn_id: i64,
    pub trigger_point: &'static str,
    pub trigger_request_id: Option<i64>,
    pub actor: Option<ActorRef>,
}

pub enum Resolution {
    Pass(TurnJudgement),
    Fired {
        intervention: uuid::Uuid,
        judgement: TurnJudgement,
    },
    WouldFire(TurnJudgement),
}

pub struct Recorder<'a> {
    pub persist: &'a PersistHandle,
    pub scope: EvaluationScope,
    pub retain_pass_verdicts: bool,
}

fn actor_of(turn: &TriggerTurn) -> Option<ActorRef> {
    turn.is_agent().then(|| ActorRef {
        persona: turn.subagent_persona.clone(),
        agent_id: turn.claude_code_agent_id.clone(),
    })
}

async fn trigger_turn(persist: &PersistHandle, target: &EvalTarget) -> Option<TriggerTurn> {
    let session_uuid: uuid::Uuid = target.session_id.parse().ok()?;
    let request_id = target.trigger_request_id?;
    match persist
        .get_turn_for_trigger_request(session_uuid, request_id)
        .await
    {
        Ok(turn) => turn,
        Err(e) => {
            debug!(request_id, error = %e, "steerer fleet: trigger turn lookup failed — not recording");
            None
        }
    }
}

async fn session_for_turn(persist: &PersistHandle, turn_id: i64) -> Option<i64> {
    persist.session_id_for_turn(turn_id).await.ok().flatten()
}

pub async fn resolve_scope(
    ctx: &FleetCtx,
    target: &EvalTarget,
    inputs: &EvalInputs,
) -> Option<EvaluationScope> {
    let persist = ctx.persist.as_ref()?;
    let turn = trigger_turn(persist, target).await;
    let operator_turn_id = inputs
        .evaluation_turn_id
        .or_else(|| turn.as_ref().and_then(|t| t.operator_turn_id));
    let Some(operator_turn_id) = operator_turn_id else {
        debug!(
            request_id = target.raw_id,
            "steerer fleet: no turn for this window — not recording"
        );
        return None;
    };
    let session_db_id = match turn.as_ref() {
        Some(t) => t.session_db_id,
        None => session_for_turn(persist, operator_turn_id).await?,
    };
    Some(EvaluationScope {
        session_db_id,
        operator_turn_id,
        trigger_point: inputs.trigger_point,
        trigger_request_id: target.trigger_request_id,
        actor: turn.as_ref().and_then(actor_of),
    })
}

fn evaluation_mode(steerer: &Steerer) -> EvaluationMode {
    match steerer.params.delivery.rollout {
        Rollout::Live => EvaluationMode::Live,
        Rollout::Shadow => EvaluationMode::Shadow,
    }
}

impl Recorder<'_> {
    pub async fn dispatch(
        &self,
        steerer: &Steerer,
        questions: &[Option<String>],
        bundle_id: Option<i64>,
    ) -> Vec<uuid::Uuid> {
        let new = NewEvaluation {
            steerer_id: steerer.id,
            session_id: self.scope.session_db_id,
            operator_turn_id: self.scope.operator_turn_id,
            mode: evaluation_mode(steerer),
            bundle_id,
            trigger_point: Some(self.scope.trigger_point).filter(|p| !p.is_empty()),
            trigger_request_id: self.scope.trigger_request_id,
        };
        match self
            .persist
            .steerer()
            .insert_evaluations_dispatched(&new, questions)
            .await
        {
            Ok(rows) => rows.into_iter().map(|(uuid, _)| uuid).collect(),
            Err(e) => {
                warn!(steerer_id = steerer.id, error = %e, "steerer fleet: evaluation dispatch failed");
                Vec::new()
            }
        }
    }

    pub async fn resolve(
        &self,
        rows: &[uuid::Uuid],
        resolution: &Resolution,
    ) {
        let (fired, intervention, judgement) = match resolution {
            Resolution::Pass(j) => (false, None, j),
            Resolution::Fired {
                intervention,
                judgement,
            } => (true, Some(*intervention), judgement),
            Resolution::WouldFire(j) => (true, None, j),
        };
        for uuid in rows {
            let record = judgement.evaluation_record(fired, self.retain_pass_verdicts);
            if let Err(e) = self
                .persist
                .steerer()
                .resolve_evaluation(*uuid, fired, intervention, record)
                .await
            {
                warn!(evaluation = %uuid, error = %e, "steerer fleet: evaluation resolve failed");
            }
        }
    }
}

pub async fn resolve_exit(
    recorder: Option<&Recorder<'_>>,
    rows: &[uuid::Uuid],
    resolution: Resolution,
) {
    if let Some(recorder) = recorder
        && !rows.is_empty()
    {
        recorder.resolve(rows, &resolution).await;
    }
}
