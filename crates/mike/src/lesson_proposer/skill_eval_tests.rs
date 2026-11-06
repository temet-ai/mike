use super::progress::{BATCH_CALLS, REVIEW_CALLS, Unit};
use super::review::{Decision, Review};
use super::trigger::{QueryResult, TriggerEval};
use super::*;

pub(super) fn material(queries: usize) -> Material {
    Material {
        name: "database-migrations".to_owned(),
        description: "Ship a migration safely.".to_owned(),
        skill_md: "---\nname: database-migrations\ndescription: \"Ship a migration safely.\"\n---\n<!-- mike-learning-generated:v1 -->\n\n# Database migrations\n\n- [references/numbering.md](references/numbering.md): read when numbering one.\n".to_owned(),
        references: vec![AuthoredReference {
            name: "numbering".to_owned(),
            when_to_read: "numbering one.".to_owned(),
            content: "# Numbering\n\nUse the next free timestamp.".to_owned(),
        }],
        queries: (0..queries)
            .map(|i| TriggerQuery {
                query: format!("query {i}"),
                should_trigger: i % 2 == 0,
            })
            .collect(),
        lesson_ids: vec![1, 2],
        agent_id: None,
    }
}

pub(super) fn result(correct: bool) -> QueryResult {
    QueryResult {
        query: "q".to_owned(),
        should_trigger: true,
        loaded: Vec::new(),
        triggered: correct,
        correct,
    }
}

pub(super) fn results(correct: usize, total: usize) -> Vec<QueryResult> {
    (0..total).map(|i| result(i < correct)).collect()
}

pub(super) fn review(decision: Decision) -> Review {
    Review {
        decision,
        reason: "checked the draft against its lessons".to_owned(),
        overlap: None,
        belongs_in: None,
        patched_description: None,
        patched_skill_md: None,
        original_description: None,
        original_skill_md: None,
    }
}

pub(super) fn patched_review() -> Review {
    Review {
        patched_description: Some("Better description.".to_owned()),
        patched_skill_md: Some("patched".to_owned()),
        original_description: Some("Ship a migration safely.".to_owned()),
        original_skill_md: Some("original".to_owned()),
        ..review(Decision::Patch)
    }
}

fn trigger_eval(correct: usize, total: usize) -> TriggerEval {
    trigger::score(results(correct, total))
}

fn draft(trigger_queries: Value) -> EvalDraft {
    EvalDraft {
        id: 1,
        project_id: 1,
        folder: "database-migrations".to_owned(),
        description: "d".to_owned(),
        skill_md: "s".to_owned(),
        reference_files: json!([]),
        trigger_queries,
        lesson_ids: vec![4, 5],
        agent_id: Some(7),
        eval_progress: None,
    }
}

#[test]
fn an_approved_draft_is_accepted_only_when_its_own_trigger_score_passes() {
    let good = verdict::decide(&trigger_eval(17, 20), None, &review(Decision::Approve));
    assert!(good.accepted, "{:?}", good.reasons);
    let at_threshold = verdict::decide(&trigger_eval(16, 20), None, &review(Decision::Approve));
    assert!(at_threshold.accepted);
    let weak = verdict::decide(&trigger_eval(15, 20), None, &review(Decision::Approve));
    assert!(!weak.accepted);
    assert_eq!(weak.reasons.len(), 1);
    assert!(weak.reasons[0].contains("the draft's trigger score 0.75 is below 0.80"));
    assert!(weak.reasons[0].contains("approved it without a patch"));
}

#[test]
fn a_patched_draft_is_decided_by_the_recheck_not_the_first_score() {
    let first = trigger_eval(10, 20);
    let rescued = verdict::decide(&first, Some(&trigger_eval(18, 20)), &patched_review());
    assert!(rescued.accepted, "{:?}", rescued.reasons);
    let still_weak = verdict::decide(&first, Some(&trigger_eval(15, 20)), &patched_review());
    assert!(!still_weak.accepted);
    assert!(still_weak.reasons[0].contains("the patched draft's trigger score 0.75"));
    let skipped = verdict::decide(&first, None, &patched_review());
    assert!(!skipped.accepted);
    let spoiled = verdict::decide(
        &trigger_eval(20, 20),
        Some(&trigger_eval(10, 20)),
        &patched_review(),
    );
    assert!(!spoiled.accepted);
}

#[test]
fn a_draft_that_adds_nothing_or_has_an_unusable_patch_is_rejected_with_the_reason() {
    let nothing = verdict::decide(
        &trigger_eval(20, 20),
        None,
        &Review {
            reason: "Nothing to save. It restates general practice.".to_owned(),
            ..review(Decision::NothingToSave)
        },
    );
    assert!(!nothing.accepted);
    assert!(nothing.reasons[0].contains("found nothing to save: Nothing to save."));
    let unusable = verdict::decide(
        &trigger_eval(20, 20),
        None,
        &Review {
            reason: "the reviewer's patch cannot ship: name must stay x".to_owned(),
            ..review(Decision::Unusable)
        },
    );
    assert!(!unusable.accepted);
    assert!(unusable.reasons[0].contains("name must stay x"));
}

#[test]
fn pending_runs_the_first_test_then_the_review_then_a_recheck_only_after_a_patch() {
    let m = material(20);
    let mut progress = Progress::fresh();
    assert_eq!(progress.pending(&m), vec![Unit::Batch(0), Unit::Batch(1)]);
    progress.trigger_batches = 1;
    assert_eq!(progress.pending(&m), vec![Unit::Batch(1)]);
    progress.trigger_batches = 2;
    assert_eq!(progress.pending(&m), vec![Unit::Review]);
    progress.review = Some(review(Decision::Approve));
    assert!(progress.pending(&m).is_empty());
    progress.review = Some(patched_review());
    assert_eq!(
        progress.pending(&m),
        vec![Unit::Recheck(0), Unit::Recheck(1)]
    );
    progress.recheck_batches = 2;
    assert!(progress.pending(&m).is_empty());
    progress.review = Some(review(Decision::NothingToSave));
    assert!(progress.pending(&m).is_empty());
}

#[test]
fn a_draft_costs_three_calls_when_approved_and_five_when_patched() {
    let m = material(20);
    let mut progress = Progress::fresh();
    assert_eq!(
        progress.worst_case_calls(&m),
        2 * BATCH_CALLS + REVIEW_CALLS + 2 * BATCH_CALLS
    );
    assert_eq!(progress.worst_case_calls(&m), 5);
    progress.trigger_batches = 2;
    assert_eq!(progress.worst_case_calls(&m), 3);
    progress.review = Some(review(Decision::Approve));
    assert_eq!(progress.worst_case_calls(&m), 0);
    progress.review = Some(patched_review());
    assert_eq!(progress.worst_case_calls(&m), 2);
    progress.recheck_batches = 1;
    assert_eq!(progress.worst_case_calls(&m), 1);
}

#[test]
fn plan_takes_the_longest_prefix_that_fits_the_granted_calls() {
    let pending = vec![Unit::Batch(0), Unit::Batch(1), Unit::Batch(2)];
    assert!(progress::plan(&pending, 0).is_empty());
    assert_eq!(
        progress::plan(&pending, 2),
        vec![Unit::Batch(0), Unit::Batch(1)]
    );
    assert_eq!(progress::plan(&pending, 100).len(), 3);
    assert_eq!(progress::cost(&progress::plan(&pending, 100)), 3);
    assert_eq!(progress::cost(&[Unit::Review, Unit::Recheck(0)]), 2);
}

#[test]
fn settled_keeps_the_results_before_the_first_error() {
    let results = vec![Ok(1), Ok(2), Err(EvalError::Model("502".to_owned())), Ok(4)];
    let (done, error) = progress::settled(results);
    assert_eq!(done, vec![1, 2]);
    assert!(matches!(error, Some(EvalError::Model(m)) if m == "502"));
    let (all, none) = progress::settled::<i32>(vec![Ok(1)]);
    assert_eq!(all, vec![1]);
    assert!(none.is_none());
}

#[test]
fn progress_round_trips_and_an_older_or_unreadable_row_starts_over() -> anyhow::Result<()> {
    let mut progress = Progress::fresh();
    progress.note_model("m1");
    progress.note_model("m1");
    progress.note_model("m2");
    progress.trigger = results(1, 2);
    progress.trigger_batches = 1;
    progress.review = Some(patched_review());
    progress.calls = 3;
    progress.siblings = Some(vec![SiblingSkill {
        folder: "ui".to_owned(),
        description: "d".to_owned(),
    }]);
    let value = serde_json::to_value(&progress)?;
    assert_eq!(Progress::from_row(Some(&value)), progress);
    assert_eq!(progress.models, vec!["m1", "m2"]);
    let mut older = value.clone();
    older["evaluator_version"] = json!(EVALUATOR_VERSION - 1);
    assert_eq!(Progress::from_row(Some(&older)), Progress::fresh());
    assert_eq!(
        Progress::from_row(Some(&json!({"trigger": "x"}))),
        Progress::fresh()
    );
    assert_eq!(Progress::from_row(None), Progress::fresh());
    Ok(())
}

#[test]
fn parse_needs_trigger_queries_and_carries_lesson_ids_and_agent() {
    let ok_queries = json!([{"query": "q", "should_trigger": true}]);
    assert!(Material::parse(&draft(ok_queries.clone())).is_ok());
    for bad in [
        draft(json!([])),
        draft(json!([{"query": "q"}])),
        draft(json!("text")),
    ] {
        assert!(matches!(Material::parse(&bad), Err(EvalError::Invalid(_))));
    }
    let carried = Material::parse(&draft(ok_queries));
    assert!(carried.is_ok_and(|m| m.lesson_ids == vec![4, 5] && m.agent_id == Some(7)));
}

#[test]
fn content_that_belongs_elsewhere_is_rejected_naming_the_place_and_the_existing_item() {
    let mut wrong = review(Decision::WrongPlace);
    wrong.belongs_in = Some(review::Place::ExistingSkill);
    wrong.overlap = Some("deploy-and-restart".to_owned());
    wrong.reason = "It is a step of the deploy procedure.".to_owned();
    let verdict = verdict::decide(&trigger_eval(20, 20), None, &wrong);
    assert!(!verdict.accepted);
    assert!(verdict.reasons[0].contains("belongs elsewhere (existing_skill: deploy-and-restart)"));
    let mut covered = review(Decision::NothingToSave);
    covered.overlap = Some("the migrations rule".to_owned());
    let verdict = verdict::decide(&trigger_eval(20, 20), None, &covered);
    assert!(
        verdict.reasons[0].contains("nothing to save (already covered by the migrations rule)")
    );
}

#[test]
fn the_named_overlap_and_place_are_stored_in_the_evaluation_and_reach_the_next_author() {
    let mut wrong = review(Decision::WrongPlace);
    wrong.belongs_in = Some(review::Place::AgentPersona);
    wrong.overlap = Some("fleet-coordinator".to_owned());
    let progress = Progress {
        trigger: results(20, 20),
        trigger_batches: 2,
        calls: 3,
        ..Progress::fresh()
    };
    let value = serde_json::to_value(evaluation(progress, wrong)).unwrap_or(Value::Null);
    assert_eq!(value["review"]["decision"], "wrong_place");
    assert_eq!(value["review"]["belongs_in"], "agent_persona");
    assert_eq!(value["review"]["overlap"], "fleet-coordinator");
    assert_eq!(value["verdict"]["accepted"], false);
    let described = crate::lesson_proposer::skill_findings::describe_eval(&value);
    assert!(described.contains("existing item: fleet-coordinator"));
    assert!(described.contains("belongs in: agent_persona"));
}

#[test]
fn an_approved_evaluation_is_stored_with_its_calls_and_no_usefulness_test() {
    let progress = Progress {
        trigger: results(17, 20),
        trigger_batches: 2,
        calls: 3,
        ..Progress::fresh()
    };
    let decided = evaluation(progress, review(Decision::Approve));
    assert!(decided.verdict.accepted, "{:?}", decided.verdict.reasons);
    let value = serde_json::to_value(&decided).unwrap_or(Value::Null);
    assert_eq!(value["evaluator_version"], EVALUATOR_VERSION);
    assert_eq!(value["calls"], 3);
    assert_eq!(value["trigger"]["correct"], 17);
    assert_eq!(value["review"]["decision"], "approve");
    assert_eq!(value["recheck"], Value::Null);
    assert_eq!(value["verdict"]["accepted"], true);
    assert!(value.get("usefulness").is_none());
    assert!(value.get("version").is_none());
}

#[test]
fn a_patched_evaluation_carries_the_recheck_and_its_findings() {
    let mut recheck = results(18, 20);
    if let Some(miss) = recheck.last_mut() {
        miss.query = "renumber my migration".to_owned();
        miss.loaded = vec!["ui-design".to_owned()];
    }
    let progress = Progress {
        trigger: results(10, 20),
        trigger_batches: 2,
        recheck,
        recheck_batches: 2,
        calls: 5,
        ..Progress::fresh()
    };
    let decided = evaluation(progress, patched_review());
    assert!(decided.verdict.accepted, "{:?}", decided.verdict.reasons);
    let value = serde_json::to_value(&decided).unwrap_or(Value::Null);
    assert_eq!(value["trigger"]["correct"], 10);
    assert_eq!(value["recheck"]["correct"], 18);
    assert_eq!(value["review"]["original_skill_md"], "original");
    assert_eq!(
        value["findings"]["missed_triggers"][1]["query"],
        "renumber my migration"
    );
    assert_eq!(
        value["findings"]["missed_triggers"][1]["loaded"],
        json!(["ui-design"])
    );
    assert_eq!(
        value["findings"]["missed_triggers"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
}

#[test]
fn a_rejected_evaluation_describes_itself_for_the_next_draft() {
    let progress = Progress {
        trigger: results(12, 20),
        trigger_batches: 2,
        calls: 3,
        ..Progress::fresh()
    };
    let decided = evaluation(progress, review(Decision::Approve));
    assert!(!decided.verdict.accepted);
    let value = serde_json::to_value(&decided).unwrap_or(Value::Null);
    let described = crate::lesson_proposer::skill_findings::describe_eval(&value);
    assert!(described.contains("Rejected because the draft's trigger score 0.60"));
    assert!(described.contains("The reviewer's decision was approve: checked the draft"));
    assert!(described.contains("Messages that should have loaded the skill and did not"));
}

#[test]
fn breaker_trips_after_three_failures_and_probes_after_the_pause() {
    let breaker = breaker::Breaker::new();
    assert!(breaker.allows(0));
    breaker.failed(100);
    breaker.failed(100);
    assert!(breaker.allows(100));
    breaker.failed(100);
    assert!(!breaker.allows(100 + breaker::PAUSE_SECS - 1));
    assert!(breaker.allows(100 + breaker::PAUSE_SECS));
    breaker.failed(100 + breaker::PAUSE_SECS);
    assert!(!breaker.allows(100 + breaker::PAUSE_SECS + 1));
    breaker.succeeded();
    assert!(breaker.allows(100 + breaker::PAUSE_SECS + 1));
}

#[test]
fn model_refusals_defer_and_other_model_errors_fail() {
    let refused = EvalError::from(LlmError::RateLimited { limit: 12 });
    assert!(matches!(refused, EvalError::Deferred(_)));
    let failed = EvalError::from(LlmError::Http { status: 502 });
    assert!(matches!(failed, EvalError::Model(_)));
}

#[test]
fn the_stored_evaluation_carries_the_project_listing_the_reviewer_and_trigger_test_saw() {
    let progress = Progress {
        trigger: results(18, 20),
        workspace: Some(workspace::Workspace {
            path: "/work/mike".to_owned(),
            available: true,
            tracked: 3,
            top_level: vec!["docs/".to_owned()],
            files: vec!["docs/SCHEMA.md".to_owned()],
        }),
        ..Progress::default()
    };
    let value = serde_json::to_value(evaluation(progress, review(Decision::Approve)))
        .unwrap_or(Value::Null);
    assert_eq!(value["workspace"]["path"], "/work/mike");
    assert_eq!(value["workspace"]["available"], true);
    assert_eq!(value["workspace"]["files"][0], "docs/SCHEMA.md");
    assert_eq!(value["evaluator_version"], json!(EVALUATOR_VERSION));
}
