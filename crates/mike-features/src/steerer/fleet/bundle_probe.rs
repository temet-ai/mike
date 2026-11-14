use super::context::EvalInputs;
use super::input_budget::VerifierInputBudget;
use super::judge::build_judge_user_message;
use super::verifier_judge::VerifierJudgeContext;
use crate::steerer::anchor::Anchor;

const LONG_PARAGRAPH: &str = "The change touches the store, the handler and the tests. \
    Every function was read before it was edited, and the gate ran after each step. ";

fn static_trigger(trigger_point: &str) -> &'static str {
    match trigger_point {
        "user_prompt" => "user_prompt",
        "turn_closed" => "turn_closed",
        "response" => "response",
        _ => "other",
    }
}

fn fixtures(trigger_point: &'static str) -> Vec<EvalInputs> {
    vec![
        EvalInputs {
            eval_text: "Done. The retry is in place and the tests pass.".to_owned(),
            operator_ask: "add a retry to the http client".to_owned(),
            last_user_message: "add a retry to the http client".to_owned(),
            trigger_point,
            turn_origin: "primary".to_owned(),
            ..Default::default()
        },
        EvalInputs {
            eval_text: "I ran the suite and fixed the two failures.".to_owned(),
            operator_ask: "fix the failing tests".to_owned(),
            last_user_message: "and push when green".to_owned(),
            tool_calls_summary:
                "Bash: cargo test (exit 1)\nEdit: src/lib.rs\nBash: cargo test (exit 0)".to_owned(),
            anchor: Some(Anchor {
                state_of_play: "tests were red".to_owned(),
                next: "make them green".to_owned(),
            }),
            trigger_point,
            turn_origin: "subagent".to_owned(),
            subagent_persona: Some("rust-developer".to_owned()),
            ..Default::default()
        },
        EvalInputs {
            eval_text: String::new(),
            operator_ask: "refactor the importer".to_owned(),
            last_user_message: "refactor the importer".to_owned(),
            trigger_point: "user_prompt",
            turn_origin: "primary_first".to_owned(),
            ..Default::default()
        },
        EvalInputs {
            eval_text: LONG_PARAGRAPH.repeat(320_000 / LONG_PARAGRAPH.len() + 1),
            operator_ask: "land the whole migration".to_owned(),
            last_user_message: "land the whole migration".to_owned(),
            trigger_point,
            turn_origin: "primary".to_owned(),
            ..Default::default()
        },
    ]
}

fn verifier_context(budget: VerifierInputBudget) -> VerifierJudgeContext {
    VerifierJudgeContext {
        accumulated_ask: "1. add a retry\n2. keep the tests green".to_owned(),
        trajectory_digest: "turn 1: edited the client\nturn 2: ran the tests".to_owned(),
        prior_fires_digest: "no prior fires".to_owned(),
        thread_criteria: Some("the retry backs off".to_owned()),
        intake_criteria: Some("tests pass".to_owned()),
        checklist_criteria: Vec::new(),
        session_action_list: "Edit src/client.rs\nBash cargo test".to_owned(),
        budget,
    }
}

pub fn steerer_input_probe(kind: &str, trigger_point: &str, verifier_budget: &str) -> String {
    let budget = if kind == "verifier" {
        VerifierInputBudget::from_setting(verifier_budget)
    } else {
        VerifierInputBudget::Uncapped
    };
    let context = (kind == "verifier").then(|| verifier_context(budget));
    fixtures(static_trigger(trigger_point))
        .iter()
        .map(|inputs| build_judge_user_message(inputs, context.as_ref(), budget))
        .collect::<Vec<String>>()
        .join("\u{1e}")
}
