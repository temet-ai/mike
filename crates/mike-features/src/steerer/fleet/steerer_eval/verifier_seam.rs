use uuid::Uuid;

use super::EvalWindow;
use super::steps::ResolvedVerdict;
use crate::llm::decisions::point::Decider;
use crate::steerer::fleet::verifier_judge::VerifierJudgeContext;
use crate::steerer::registry::Steerer;
use crate::steerer::verifier_precheck::commits::recent_commits;
use crate::steerer::verifier_precheck::{self, Checked, Facts, FireInput, POINT, Sources, Subject};
use crate::store::persistence::PersistHandle;

pub struct Precheck {
    decider: Decider,
    checked: Checked,
}

impl Precheck {
    pub fn holds(&self) -> bool {
        self.checked.verdict.holds()
    }

    pub async fn mark(&self, delivered: bool) {
        verifier_precheck::mark(&self.decider, &self.checked, delivered).await;
    }
}

async fn facts(
    window: &EvalWindow<'_>,
    persist: &PersistHandle,
    nudge: &str,
    resolved: &ResolvedVerdict,
    context: Option<&VerifierJudgeContext>,
) -> Facts {
    let criteria: Vec<&str> = context
        .map(|c| {
            c.checklist_criteria
                .iter()
                .map(|k| k.text.as_str())
                .collect()
        })
        .unwrap_or_default();
    let sources = Sources {
        nudge,
        verdict: &resolved.verdict,
        criteria: &criteria,
        accumulated_ask: context.map_or("", |c| c.accumulated_ask.as_str()),
        operator_ask: &window.eval_inputs.operator_ask,
        last_user_message: &window.eval_inputs.last_user_message,
    };
    let commits = match window.scope() {
        Some(scope) => recent_commits(persist, scope.session_db_id).await,
        None => Vec::new(),
    };
    Facts::derive(&sources, commits)
}

pub async fn run(
    window: &EvalWindow<'_>,
    steerer: &Steerer,
    resolved: &ResolvedVerdict,
    nudge: &str,
    rows: &[Uuid],
    context: Option<&VerifierJudgeContext>,
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
    let facts = facts(window, persist, nudge, resolved, context).await;
    let scope = window.scope();
    let subject = Subject {
        session_db_id: scope.map(|s| s.session_db_id),
        operator_turn_id: scope.map(|s| s.operator_turn_id),
        request_id: window.target.trigger_request_id,
        evaluation: rows.first().copied(),
    };
    let checked = verifier_precheck::check(&decider, &input, &facts, &subject).await?;
    Some(Precheck { decider, checked })
}
