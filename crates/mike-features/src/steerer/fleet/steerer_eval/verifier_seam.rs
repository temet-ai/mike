use uuid::Uuid;

use super::EvalWindow;
use super::steps::ResolvedVerdict;
use crate::llm::decisions::point::Decider;
use crate::steerer::registry::Steerer;
use crate::steerer::verifier_precheck::{self, Checked, FireInput, POINT, Subject};

pub struct Precheck {
    decider: Decider,
    checked: Checked,
}

impl Precheck {
    pub fn holds(&self) -> bool {
        !self.checked.verdict.send
    }

    pub async fn mark(&self, delivered: bool) {
        verifier_precheck::mark(&self.decider, &self.checked, delivered).await;
    }
}

pub async fn run(
    window: &EvalWindow<'_>,
    steerer: &Steerer,
    resolved: &ResolvedVerdict,
    nudge: &str,
    rows: &[Uuid],
) -> Option<Precheck> {
    if steerer.kind != "verifier" {
        return None;
    }
    let persist = window.ctx.persist.as_ref()?;
    let decider = Decider::for_point(
        window.base_llm,
        persist,
        window.ctx.toggles.load().decisions,
        &window.ctx.settings.load(),
        POINT,
    )?;
    let input = FireInput {
        operator_ask: &window.eval_inputs.operator_ask,
        turn_end: &window.eval_inputs.eval_text,
        nudge,
        verdict: &resolved.verdict,
    };
    let scope = window.scope();
    let subject = Subject {
        session_db_id: scope.map(|s| s.session_db_id),
        operator_turn_id: scope.map(|s| s.operator_turn_id),
        request_id: window.target.trigger_request_id,
        evaluation: rows.first().copied(),
    };
    let checked = verifier_precheck::check(&decider, &input, &subject).await?;
    Some(Precheck { decider, checked })
}
