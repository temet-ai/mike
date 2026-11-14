use super::{EvalWindow, WindowState};
use crate::llm::decisions::point::Decider;
use crate::llm::decisions::record::DecisionSubject;
use crate::steerer::checklist_point::{self, Checked, Evidence};
use crate::steerer::registry::Steerer;
use crate::steerer::verdict::SteererVerdict;

pub struct JevChecklist {
    decider: Decider,
    checked: Checked,
}

fn decider_for(window: &EvalWindow<'_>) -> Option<Decider> {
    let persist = window.ctx.persist.as_ref()?;
    Decider::for_point(
        window.base_llm,
        persist,
        window.ctx.toggles.load().decisions,
        &window.ctx.settings.load(),
        checklist_point::POINT,
    )
}

pub async fn run(
    window: &EvalWindow<'_>,
    steerer: &Steerer,
    state: &WindowState<'_>,
) -> Option<JevChecklist> {
    if steerer.kind != "verifier" {
        return None;
    }
    let context = state.verifier_context.as_ref()?;
    let criteria = &context.checklist_criteria;
    if criteria.is_empty() {
        return None;
    }
    let decider = decider_for(window)?;
    let scope = window.scope();
    let subject = DecisionSubject {
        session_id: scope.map(|s| s.session_db_id),
        operator_turn_id: scope.map(|s| s.operator_turn_id),
        request_id: window.target.trigger_request_id,
        ..DecisionSubject::default()
    };
    let evidence = Evidence {
        trajectory: &context.trajectory_digest,
        turn_end: &window.eval_inputs.eval_text,
    };
    let checked = checklist_point::check(&decider, criteria, &evidence, subject).await?;
    Some(JevChecklist { decider, checked })
}

impl JevChecklist {
    pub fn fires(&self) -> bool {
        self.checked.checklist.fires()
    }

    pub async fn no_fire(&self) -> SteererVerdict {
        checklist_point::mark(&self.decider, &self.checked).await;
        self.checked.checklist.verdict()
    }

    pub async fn fire_with(&self, written: &SteererVerdict) -> SteererVerdict {
        checklist_point::mark(&self.decider, &self.checked).await;
        checklist_point::label_against_writer(&self.decider, &self.checked, written.fires).await;
        self.checked.checklist.merged(written)
    }
}
