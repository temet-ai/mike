use super::ReplayItem;
use super::run::ReplaySteerer;
use crate::steerer::anchor::Anchor;
use crate::steerer::fleet::context::EvalInputs;
use crate::steerer::fleet::input_budget::{
    RawVerifierContext, VerifierInputBudget, project_prior_fires, project_verifier_context,
};
use crate::steerer::fleet::judge::build_judge_user_message;
use crate::steerer::fleet::session_block::{BlockRequest, session_block, with_session_block};
use crate::steerer::fleet::triggers::response_trigger::resolve_operator_ask_with;
use crate::steerer::fleet::verifier_judge::{VerifierJudgeContext, render_intake_criteria};
use crate::steerer::gate::strip_harness_scaffold;
use crate::steerer::note_policy::SessionNotes;
use crate::steerer::verifier::{
    build_accumulated_ask, build_prior_fires_digest, build_trajectory_digest, summarise_tool_calls,
    written_by_the_operator,
};
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::steerer_repo::SteererPersist as _;
use crate::store::persistence::steering::PriorFireRow;
use crate::store::persistence::turn_tool_call_reads::TurnToolCallMetaRow;
use crate::store::persistence::verifier_replay::ReplayTrigger;
use crate::store::persistence::{PersistError, PersistHandle};
use crate::types::ToolCall;

const LEDGER_FETCH_LIMIT: i64 = 30;
const PRIOR_FIRES_LIMIT: i64 = 10;

pub(super) struct AsOfInputs {
    pub(super) eval_inputs: EvalInputs,
    pub(super) context: VerifierJudgeContext,
    pub(super) session_block: String,
    pub(super) notes: SessionNotes,
    pub(super) user_message: String,
}

async fn load_eval_inputs(
    ph: &PersistHandle,
    item: &ReplayItem,
    trigger: &ReplayTrigger,
) -> Result<EvalInputs, PersistError> {
    let blocks = ph
        .steerer()
        .fetch_replay_eval_text_blocks(item.trigger_request_id)
        .await?;
    let uses = ph
        .steerer()
        .fetch_replay_trigger_tool_uses(item.trigger_request_id)
        .await?;
    let tool_calls: Vec<ToolCall> = uses
        .into_iter()
        .map(|u| ToolCall {
            tool_id: String::new(),
            name: u.tool_name,
            input_json: u.input.map(|v| v.to_string()).unwrap_or_default(),
            complete: true,
        })
        .collect();
    let anchor = ph
        .steerer()
        .get_session_goal(trigger.session_uuid)
        .await?
        .map(|goal| Anchor {
            state_of_play: goal.goal,
            next: goal.next_steps,
        });
    let operator_ask = resolve_operator_ask_with(&trigger.human_prompt, || {
        ph.steerer()
            .fetch_open_turn_prompt_as_of(trigger.session_db_id, trigger.trigger_at)
    })
    .await?;
    Ok(EvalInputs {
        eval_text: blocks.concat(),
        operator_ask,
        last_user_message: strip_harness_scaffold(&trigger.last_user_message),
        tool_calls_summary: summarise_tool_calls(&tool_calls),
        anchor,
        trigger_point: "response",
        turn_origin: "primary".to_owned(),
        judged_turn_id: Some(item.turn_id),
        ..Default::default()
    })
}

async fn load_action_rows(
    ph: &PersistHandle,
    trigger: &ReplayTrigger,
    budget: VerifierInputBudget,
) -> Result<RawActions, PersistError> {
    let Some(limit) = budget.action_rows_loaded() else {
        let rows = ph
            .steerer()
            .fetch_session_action_list_as_of(trigger.session_db_id, trigger.trigger_at)
            .await?;
        return Ok((rows, None));
    };
    let rows = ph
        .steerer()
        .fetch_session_action_list_recent_as_of(trigger.session_db_id, trigger.trigger_at, limit)
        .await?;
    let counts = ph
        .steerer()
        .fetch_session_action_counts_as_of(trigger.session_db_id, trigger.trigger_at)
        .await?;
    Ok((rows, Some(counts)))
}

type RawActions = (
    Vec<crate::store::persistence::session_action_list::SessionActionRow>,
    Option<crate::store::persistence::session_action_list::SessionActionCounts>,
);

async fn load_intake_criteria(
    ph: &PersistHandle,
    trigger: &ReplayTrigger,
) -> Result<Option<String>, PersistError> {
    let map = ph
        .steerer()
        .fetch_acceptance_criteria_as_of(trigger.session_db_id, trigger.trigger_at)
        .await?;
    Ok(map
        .filter(|m| !m.criteria.is_empty())
        .map(|m| render_intake_criteria(&m)))
}

async fn load_raw_context(
    ph: &PersistHandle,
    item: &ReplayItem,
    trigger: &ReplayTrigger,
    budget: VerifierInputBudget,
) -> Result<RawVerifierContext, PersistError> {
    let ask_entries = written_by_the_operator(
        ph.steerer()
            .fetch_ask_ledger_as_of(
                trigger.session_db_id,
                trigger.trigger_at,
                LEDGER_FETCH_LIMIT,
            )
            .await?,
    );
    let calls = ph
        .steerer()
        .fetch_turn_tool_calls_as_of(item.turn_id, trigger.session_db_id, trigger.trigger_at)
        .await?;
    let metas: Vec<TurnToolCallMetaRow> = calls.into_iter().map(Into::into).collect();
    let (action_rows, action_counts) = load_action_rows(ph, trigger, budget).await?;
    let thread_criteria = ph
        .launch()
        .fetch_thread_criteria_for_session(
            trigger.session_db_id,
            &build_accumulated_ask(&ask_entries),
        )
        .await?
        .map(|thread| thread.criteria_text);
    let intake_criteria = if thread_criteria.is_some() {
        None
    } else {
        load_intake_criteria(ph, trigger).await?
    };
    Ok(RawVerifierContext {
        ask_entries,
        trajectory_digest: build_trajectory_digest(&metas),
        action_rows,
        action_counts,
        thread_criteria,
        intake_criteria,
    })
}

async fn load_prior_fires_digest(
    ph: &PersistHandle,
    item: &ReplayItem,
    trigger: &ReplayTrigger,
    steerer_id: i64,
) -> Result<String, PersistError> {
    let rows = ph
        .steerer()
        .fetch_prior_verifier_fires_as_of(
            trigger.session_db_id,
            steerer_id,
            trigger.trigger_at,
            PRIOR_FIRES_LIMIT,
            item.trigger_request_id,
        )
        .await?;
    let fires: Vec<PriorFireRow> = rows
        .into_iter()
        .map(|row| PriorFireRow {
            off_thread_topic: row.off_thread_topic,
            created_at: row.created_at,
        })
        .collect();
    Ok(build_prior_fires_digest(&fires))
}

pub(super) async fn assemble(
    ph: &PersistHandle,
    item: &ReplayItem,
    replay: &ReplaySteerer,
    budget: VerifierInputBudget,
) -> Result<AsOfInputs, PersistError> {
    let steerer_id = replay.steerer.id;
    let limits = replay.note_limits;
    let trigger = ph
        .steerer()
        .fetch_replay_trigger(item.trigger_request_id)
        .await?;
    let eval_inputs = load_eval_inputs(ph, item, &trigger).await?;
    let raw = load_raw_context(ph, item, &trigger, budget).await?;
    let prior_fires = load_prior_fires_digest(ph, item, &trigger, steerer_id).await?;
    let mut context = project_verifier_context(&raw, budget);
    context.prior_fires_digest = project_prior_fires(prior_fires, budget);
    let block_request = BlockRequest::at(
        trigger.session_db_id,
        item.turn_id,
        limits.window,
        trigger.trigger_at,
    );
    let session_block = session_block(ph, &block_request).await;
    let notes = SessionNotes::load(ph, trigger.session_db_id, limits, trigger.trigger_at).await;
    let user_message = with_session_block(
        build_judge_user_message(&eval_inputs, Some(&context), budget),
        &session_block,
    );
    Ok(AsOfInputs {
        eval_inputs,
        context,
        session_block,
        notes,
        user_message,
    })
}
