use super::context::TurnContext;
use super::{GuardTurn, Question};
use crate::llm::OpenRouterClient;
use crate::llm::decisions::point::Decider;
use crate::llm::decisions::record::DecisionSubject;
use crate::steerer::{ended_on_point, step_class_point};
use crate::store::persistence::PersistHandle;

#[derive(Clone, Default)]
pub struct GuardDeciders {
    pub ended_on: Option<Decider>,
    pub step_class: Option<Decider>,
}

impl GuardDeciders {
    pub fn for_turn(
        base: &OpenRouterClient,
        persist: &PersistHandle,
        toggle_on: bool,
        overlay: &crate::settings::SettingsOverlay,
    ) -> Self {
        Self {
            ended_on: Decider::for_point(base, persist, toggle_on, overlay, ended_on_point::POINT),
            step_class: Decider::for_point(
                base,
                persist,
                toggle_on,
                overlay,
                step_class_point::POINT,
            ),
        }
    }
}

fn subject(turn: &GuardTurn<'_>) -> DecisionSubject {
    DecisionSubject {
        session_id: Some(turn.session_db_id),
        operator_turn_id: Some(turn.operator_turn_id),
        request_id: turn.trigger_request_id,
        ..DecisionSubject::default()
    }
}

fn reads_the_ending(open: &[Question]) -> bool {
    open.iter().any(|q| {
        matches!(
            q,
            Question::UnbackedWait | Question::AuthorisedHandback | Question::UnobservedDone
        )
    })
}

pub async fn refine_phrase(turn: &GuardTurn<'_>, ctx: &mut TurnContext, open: &[Question]) {
    if reads_the_ending(open)
        && !ctx.phrase.empty
        && let Some(decider) = turn.jev.ended_on.as_ref()
    {
        ctx.phrase =
            ended_on_point::refine(decider, ctx.phrase.clone(), &ctx.ending, subject(turn)).await;
    }
    if open.contains(&Question::AuthorisedHandback)
        && ctx.phrase.g3
        && let Some(decider) = turn.jev.step_class.as_ref()
    {
        ctx.phrase.step_class = step_class_point::pick(decider, &ctx.ending, subject(turn)).await;
    }
}
