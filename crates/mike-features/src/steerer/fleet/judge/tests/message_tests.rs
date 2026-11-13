use super::super::*;

#[test]
fn build_judge_user_message_response_path() {
    let inputs = EvalInputs {
        eval_text: "I have finished the task.".to_string(),
        operator_ask: "Please finish the task.".to_string(),
        last_user_message: "Please finish the task.".to_string(),
        tool_calls_summary: String::new(),
        diagnostics: Vec::new(),
        anchor: None,
        trigger_point: "response",
        turn_origin: String::new(),
        subagent_persona: None,
        judged_turn_id: None,
        evaluation_turn_id: None,
        question: None,
    };
    let msg = build_judge_user_message(&inputs, None, VerifierInputBudget::Uncapped);
    assert!(msg.contains("User ask:"));
    assert!(msg.contains("Please finish the task."));
    assert!(msg.contains("Assistant response to evaluate:"));
    assert!(msg.contains("I have finished the task."));
}

#[test]
fn build_judge_user_message_tool_loop_uses_resolved_operator_ask() {
    let inputs = EvalInputs {
        eval_text: "Ran the tests, all green.".to_string(),
        operator_ask: "Refactor the auth module and add tests.".to_string(),
        last_user_message: String::new(),
        trigger_point: "response",
        ..Default::default()
    };
    let msg = build_judge_user_message(&inputs, None, VerifierInputBudget::Uncapped);
    assert!(msg.contains("User ask:\nRefactor the auth module and add tests."));
    assert!(!msg.contains("Latest message:"));
}

#[test]
fn build_judge_user_message_differing_last_message_shown_separately() {
    let inputs = EvalInputs {
        eval_text: "Done.".to_string(),
        operator_ask: "Refactor the auth module.".to_string(),
        last_user_message: "please also check the linter".to_string(),
        trigger_point: "response",
        ..Default::default()
    };
    let msg = build_judge_user_message(&inputs, None, VerifierInputBudget::Uncapped);
    assert!(msg.contains("User ask:\nRefactor the auth module."));
    assert!(msg.contains("Latest message:\nplease also check the linter"));
}

#[test]
fn build_judge_user_message_user_prompt_path() {
    let inputs = EvalInputs {
        eval_text: "How do I refactor this?".to_string(),
        operator_ask: "How do I refactor this?".to_string(),
        last_user_message: "How do I refactor this?".to_string(),
        tool_calls_summary: String::new(),
        diagnostics: Vec::new(),
        anchor: None,
        trigger_point: "user_prompt",
        turn_origin: String::new(),
        subagent_persona: None,
        judged_turn_id: None,
        evaluation_turn_id: None,
        question: None,
    };
    let msg = build_judge_user_message(&inputs, None, VerifierInputBudget::Uncapped);
    assert!(msg.contains("User prompt to evaluate:"));
    assert!(!msg.contains("Assistant response to evaluate:"));
}

#[test]
fn build_judge_user_message_primary_turn_names_main_orchestrator() {
    let inputs = EvalInputs {
        eval_text: "Edited handler.rs directly.".to_string(),
        operator_ask: "fix the bug".to_string(),
        last_user_message: "fix the bug".to_string(),
        tool_calls_summary: "1. Edit — crates/mike/src/api/handlers.rs\n".to_string(),
        diagnostics: Vec::new(),
        anchor: None,
        trigger_point: "response",
        turn_origin: "primary".to_string(),
        subagent_persona: None,
        judged_turn_id: None,
        evaluation_turn_id: None,
        question: None,
    };
    let msg = build_judge_user_message(&inputs, None, VerifierInputBudget::Uncapped);
    assert!(msg.contains("Acting agent: main orchestrator (not a specialist subagent)"));
    assert!(!msg.contains("subagent ·"));
}

#[test]
fn build_judge_user_message_subagent_turn_names_persona() {
    let inputs = EvalInputs {
        eval_text: "Edited handlers.rs on the specialist's behalf.".to_string(),
        operator_ask: "fix the bug".to_string(),
        last_user_message: "fix the bug".to_string(),
        tool_calls_summary: "1. Edit — crates/mike/src/api/handlers.rs\n".to_string(),
        diagnostics: Vec::new(),
        anchor: None,
        trigger_point: "response",
        turn_origin: "subagent".to_string(),
        subagent_persona: Some("rust-developer".to_string()),
        judged_turn_id: None,
        evaluation_turn_id: None,
        question: None,
    };
    let msg = build_judge_user_message(&inputs, None, VerifierInputBudget::Uncapped);
    assert!(msg.contains("Acting agent: subagent · rust-developer"));

    let inputs_no_persona = EvalInputs {
        subagent_persona: None,
        ..inputs
    };
    let msg_no_persona =
        build_judge_user_message(&inputs_no_persona, None, VerifierInputBudget::Uncapped);
    assert!(msg_no_persona.contains("Acting agent: subagent · unknown specialist"));
}

#[test]
fn build_judge_user_message_verifier_context_adds_checklist_instruction() {
    let inputs = EvalInputs {
        eval_text: "Done, all tests pass.".to_string(),
        operator_ask: "Refactor the auth module and add tests.".to_string(),
        last_user_message: "Refactor the auth module and add tests.".to_string(),
        trigger_point: "response",
        ..Default::default()
    };
    let generic_msg = build_judge_user_message(&inputs, None, VerifierInputBudget::Uncapped);
    assert!(!generic_msg.contains("Accumulated ask"));
    assert!(!generic_msg.contains("checklist"));

    let judge_context = VerifierJudgeContext {
        accumulated_ask: "[initial] Refactor the auth module and add tests.".to_string(),
        trajectory_digest: "1. [req 100] Edit (write) → ok [tool t1]\n".to_string(),
        prior_fires_digest: String::new(),
        thread_criteria: None,
        intake_criteria: None,
        session_action_list: String::new(),
        ..Default::default()
    };
    let msg =
        build_judge_user_message(&inputs, Some(&judge_context), VerifierInputBudget::Uncapped);
    assert!(msg.contains("Accumulated ask"));
    assert!(msg.contains("[initial] Refactor the auth module and add tests."));
    assert!(msg.contains("Trajectory digest"));
    assert!(msg.contains("[req 100] Edit (write) → ok"));
    assert!(msg.contains("populate `checklist`"));
    assert!(!msg.contains("fire memory"));
}

#[test]
fn build_judge_user_message_thread_criteria_overrides_generated_checklist() {
    let inputs = EvalInputs {
        eval_text: "Done, all tests pass.".to_string(),
        operator_ask: "Refactor the auth module and add tests.".to_string(),
        last_user_message: "Refactor the auth module and add tests.".to_string(),
        trigger_point: "response",
        ..Default::default()
    };
    let judge_context = VerifierJudgeContext {
        accumulated_ask: "[initial] Refactor the auth module and add tests.".to_string(),
        trajectory_digest: "1. [req 100] Edit (write) → ok [tool t1]\n".to_string(),
        prior_fires_digest: String::new(),
        thread_criteria: Some("1. Ship the refactor\n2. Prove it with tests".to_string()),
        intake_criteria: None,
        session_action_list: String::new(),
        ..Default::default()
    };
    let msg =
        build_judge_user_message(&inputs, Some(&judge_context), VerifierInputBudget::Uncapped);
    assert!(msg.contains("Operator-approved acceptance criteria"));
    assert!(msg.contains("1. Ship the refactor\n2. Prove it with tests"));
    assert!(msg.contains("do not invent additional criteria"));
    assert!(!msg.contains("decompose it into 5-15"));
    assert!(msg.contains("Accumulated ask"));
}

#[test]
fn build_judge_user_message_intake_criteria_replace_the_generated_checklist() {
    let inputs = EvalInputs {
        eval_text: "Done, the panel is in.".to_string(),
        operator_ask: "add a comms panel".to_string(),
        last_user_message: "add a comms panel".to_string(),
        trigger_point: "response",
        ..Default::default()
    };
    let judge_context = VerifierJudgeContext {
        accumulated_ask: "[initial] add a comms panel".to_string(),
        trajectory_digest: "1. [req 100] Write → ok [tool t1]\n".to_string(),
        prior_fires_digest: String::new(),
        thread_criteria: None,
        intake_criteria: Some(
            "- [operator asked] The comms panel is reachable from the session page.\n  \
                 (their words: \"add a comms panel\")"
                .to_string(),
        ),
        session_action_list: String::new(),
        ..Default::default()
    };
    let msg =
        build_judge_user_message(&inputs, Some(&judge_context), VerifierInputBudget::Uncapped);
    assert!(msg.contains("authored at intake from the operator's own words"));
    assert!(msg.contains("The comms panel is reachable from the session page."));
    assert!(msg.contains("do not invent additional criteria"));
    assert!(
        !msg.contains("decompose it into 5-15"),
        "the generated checklist must not also be requested"
    );
}

#[test]
fn build_judge_user_message_thread_criteria_beat_the_intake_map() {
    let inputs = EvalInputs {
        eval_text: "Done.".to_string(),
        operator_ask: "ship it".to_string(),
        last_user_message: "ship it".to_string(),
        trigger_point: "response",
        ..Default::default()
    };
    let judge_context = VerifierJudgeContext {
        accumulated_ask: "[initial] ship it".to_string(),
        trajectory_digest: String::new(),
        prior_fires_digest: String::new(),
        thread_criteria: Some("OPERATOR_APPROVED_CRITERION".to_string()),
        intake_criteria: Some("INFERRED_INTAKE_CRITERION".to_string()),
        session_action_list: String::new(),
        ..Default::default()
    };
    let msg =
        build_judge_user_message(&inputs, Some(&judge_context), VerifierInputBudget::Uncapped);
    assert!(msg.contains("OPERATOR_APPROVED_CRITERION"));
    assert!(
        !msg.contains("INFERRED_INTAKE_CRITERION"),
        "an operator-approved worklog must not be diluted by an inferred map"
    );
}

#[test]
fn build_judge_user_message_tells_the_judge_what_inferred_means() {
    let inputs = EvalInputs {
        eval_text: "Done.".to_string(),
        operator_ask: "x".to_string(),
        last_user_message: "x".to_string(),
        trigger_point: "response",
        ..Default::default()
    };
    let judge_context = VerifierJudgeContext {
        accumulated_ask: "[initial] x".to_string(),
        trajectory_digest: String::new(),
        prior_fires_digest: String::new(),
        thread_criteria: None,
        intake_criteria: Some("- [inferred, not stated by the operator] It renders.".to_string()),
        session_action_list: String::new(),
        ..Default::default()
    };
    let msg =
        build_judge_user_message(&inputs, Some(&judge_context), VerifierInputBudget::Uncapped);
    assert!(msg.contains("was NOT stated by the operator"));
    assert!(msg.contains("do not fire on an inferred criterion alone"));
}

#[test]
fn build_judge_user_message_verifier_context_renders_fire_memory() {
    let inputs = EvalInputs {
        eval_text: "Done.".to_string(),
        operator_ask: "fix it".to_string(),
        trigger_point: "response",
        ..Default::default()
    };
    let judge_context = VerifierJudgeContext {
        accumulated_ask: String::new(),
        trajectory_digest: String::new(),
        prior_fires_digest: "1. 2026-08-05 09:00:00Z — claimed done, no evidence".to_string(),
        thread_criteria: None,
        intake_criteria: None,
        session_action_list: String::new(),
        ..Default::default()
    };
    let msg =
        build_judge_user_message(&inputs, Some(&judge_context), VerifierInputBudget::Uncapped);
    assert!(msg.contains("fire memory"));
    assert!(msg.contains("claimed done, no evidence"));
    assert!(msg.contains("do not re-fire on the same already-flagged gap"));
}

#[test]
fn build_judge_user_message_verifier_context_empty_fields_add_no_blocks() {
    let inputs = EvalInputs {
        eval_text: "Done.".to_string(),
        operator_ask: "fix it".to_string(),
        trigger_point: "response",
        ..Default::default()
    };
    let judge_context = VerifierJudgeContext::default();
    let msg =
        build_judge_user_message(&inputs, Some(&judge_context), VerifierInputBudget::Uncapped);
    assert!(!msg.contains("Accumulated ask"));
    assert!(!msg.contains("Trajectory digest"));
    assert!(msg.contains("populate `checklist`"));
}
