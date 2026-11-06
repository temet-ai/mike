use chrono::Utc;

use super::gates::{EndedOn, phrase_gates};
use super::question_gates::{
    GateContext, PriorReaction, done_gate, handback_gate, relapse_gate, wait_gate,
};
use super::step_class::DirectiveInForce;
use super::{Question, Trigger};
use crate::dossier::steering_facts::SteeringFacts;
use crate::dossier::steering_facts::observation::Observation;
use crate::dossier::steering_facts::wait_backing::WaitBacking;

fn facts(backing: WaitBacking, observed: Option<bool>) -> SteeringFacts {
    SteeringFacts {
        wait_backing: backing,
        wait_backing_refs: Vec::new(),
        observation: Observation {
            observed_after_change: observed,
            observation_kind: observed.filter(|o| *o).map(|_| "test"),
            ..Observation::default()
        },
        session_class: None,
        as_of: Utc::now(),
        version: 1,
    }
}

fn prior(turn_id: i64, reaction: Option<&str>) -> PriorReaction {
    PriorReaction {
        turn_id,
        reaction: reaction.map(str::to_owned),
        verdict_id: reaction.map(|_| turn_id * 10),
        rationale: Some("the operator said stop waiting".to_owned()),
    }
}

struct Case {
    text: &'static str,
    facts: Option<SteeringFacts>,
    prior: Vec<PriorReaction>,
    live: bool,
    trigger: Trigger,
    idle: Option<i64>,
    directives: Vec<DirectiveInForce>,
}

impl Case {
    fn new(text: &'static str) -> Self {
        Self {
            text,
            facts: Some(facts(WaitBacking::None, Some(false))),
            prior: Vec::new(),
            live: true,
            trigger: Trigger::TurnEnd,
            idle: None,
            directives: Vec::new(),
        }
    }

    fn check(&self, question: Question) -> (bool, String) {
        let phrase = phrase_gates(self.text, "");
        let ctx = GateContext {
            phrase: &phrase,
            trigger: self.trigger,
            facts: self.facts.as_ref(),
            prior: &self.prior,
            open_turn_id: 30,
            turn_judge_live: self.live,
            last_tool: None,
            directives: &self.directives,
            idle_seconds: self.idle,
            quiescence_after_secs: 90,
            ending: self.text,
        };
        let g = match question {
            Question::Relapse => relapse_gate(&ctx),
            Question::UnbackedWait => wait_gate(&ctx),
            Question::AuthorisedHandback => handback_gate(&ctx),
            Question::UnobservedDone => done_gate(&ctx),
        };
        (g.outcome.open, g.outcome.reason)
    }
}

#[test]
fn gate_g2_matches_d_py_fixture_wait_phrases() {
    for text in [
        "Build started. I'll report back when it lands.",
        "Waiting for the CI run to finish.",
        "The job is still running in the background.",
        "Let me check the logs next",
        "Next steps:",
        "Running the migration...",
    ] {
        assert!(phrase_gates(text, "").g2, "{text}");
    }
    for text in [
        "The migration ran and every test passed.",
        "Here is the diff.",
    ] {
        assert!(!phrase_gates(text, "").g2, "{text}");
    }
}

#[test]
fn gate_g3_matches_d_py_fixture_handbacks() {
    assert!(phrase_gates("Want me to push it?", "").g3);
    assert!(phrase_gates("It needs your approval before I merge", "").g3);
    assert!(phrase_gates("All green.\nAnything else?", "").g3);
    assert!(!phrase_gates("I pushed it.", "").g3);
}

#[test]
fn gate_g4_matches_d_py_fixture_done_words() {
    assert!(phrase_gates("The fix is done.", "").g4);
    assert!(phrase_gates("Deployed and verified", "").g4);
    assert!(!phrase_gates("Undone work remains", "").g4, "word-bounded");
    assert!(
        phrase_gates("looks good to ship", "looks good to ship").g4,
        "verifier pattern lines"
    );
}

#[test]
fn ended_on_order_matches_d_py() {
    assert_eq!(
        phrase_gates("Shall I commit this?", "").ended_on,
        EndedOn::Handback
    );
    assert_eq!(
        phrase_gates("Is the port 9988?", "").ended_on,
        EndedOn::Question
    );
    assert_eq!(
        phrase_gates("I'll report back soon.", "").ended_on,
        EndedOn::PromiseOrWait
    );
    assert_eq!(
        phrase_gates("The fix is done.", "").ended_on,
        EndedOn::Report
    );
    assert_eq!(
        phrase_gates("Here is the table.", "").ended_on,
        EndedOn::Other
    );
}

#[test]
fn g2_closed_when_backed() {
    let case = Case {
        facts: Some(facts(WaitBacking::WorkItem, None)),
        ..Case::new("I'll report back when it lands.")
    };
    let (open, reason) = case.check(Question::UnbackedWait);
    assert!(!open);
    assert_eq!(reason, "backed:work_item");
}

#[test]
fn g2_closed_when_backing_unknown() {
    let case = Case {
        facts: Some(facts(WaitBacking::Unknown, None)),
        ..Case::new("I'll report back when it lands.")
    };
    assert_eq!(case.check(Question::UnbackedWait).1, "backing_unknown");
}

#[test]
fn g2_arms_recorded_separately() {
    let turn_end = Case::new("I'll report back when it lands.");
    assert_eq!(
        turn_end.check(Question::UnbackedWait),
        (true, "open".to_owned())
    );
    let quiet = Case {
        trigger: Trigger::Quiescence,
        idle: Some(120),
        ..Case::new("Here is the table.")
    };
    assert!(quiet.check(Question::UnbackedWait).0);
}

#[test]
fn g2b_opens_on_other_ending_unbacked_after_quiet() {
    let case = Case {
        trigger: Trigger::Quiescence,
        idle: Some(120),
        ..Case::new("Here is the table.")
    };
    assert!(case.check(Question::UnbackedWait).0);
}

#[test]
fn g2b_opens_on_report_ending_unbacked_after_quiet() {
    let case = Case {
        trigger: Trigger::Quiescence,
        idle: Some(95),
        ..Case::new("The fix is done.")
    };
    assert!(case.check(Question::UnbackedWait).0);
}

#[test]
fn g2b_closed_on_phrase_ending() {
    let case = Case {
        trigger: Trigger::Quiescence,
        idle: Some(120),
        ..Case::new("Shall I commit this?")
    };
    let (open, reason) = case.check(Question::UnbackedWait);
    assert!(!open);
    assert_eq!(reason, "question_ending");
}

#[test]
fn g2b_closed_when_not_quiet() {
    let case = Case {
        trigger: Trigger::Quiescence,
        idle: Some(30),
        ..Case::new("Here is the table.")
    };
    assert_eq!(
        case.check(Question::UnbackedWait),
        (false, "no_wait_ending".to_owned())
    );
}

#[test]
fn g2b_closed_when_backed() {
    let case = Case {
        trigger: Trigger::Quiescence,
        idle: Some(120),
        facts: Some(facts(WaitBacking::Monitor, None)),
        ..Case::new("Here is the table.")
    };
    assert!(!case.check(Question::UnbackedWait).0);
}

#[test]
fn g4_closed_when_observed() {
    let case = Case {
        facts: Some(facts(WaitBacking::None, Some(true))),
        ..Case::new("The fix is done.")
    };
    assert_eq!(
        case.check(Question::UnobservedDone),
        (false, "observed:test".to_owned())
    );
}

#[test]
fn g4_closed_when_no_change() {
    let case = Case {
        facts: Some(facts(WaitBacking::None, None)),
        ..Case::new("The fix is done.")
    };
    assert_eq!(case.check(Question::UnobservedDone).1, "no_change");
    assert!(
        Case::new("The fix is done.")
            .check(Question::UnobservedDone)
            .0
    );
}

#[test]
fn g1_closed_without_live_reaction() {
    let case = Case {
        live: false,
        prior: vec![prior(29, Some("corrected"))],
        ..Case::new("ok")
    };
    assert_eq!(case.check(Question::Relapse).1, "no_live_reaction");
}

#[test]
fn g1_open_on_corrected_previous_turn() {
    let case = Case {
        prior: vec![prior(28, Some("accepted")), prior(29, Some("corrected"))],
        ..Case::new("ok")
    };
    assert!(case.check(Question::Relapse).0);
}

#[test]
fn g1_open_two_back_without_accepted_between() {
    let case = Case {
        prior: vec![prior(28, Some("corrected")), prior(29, Some("redirected"))],
        ..Case::new("ok")
    };
    assert!(case.check(Question::Relapse).0);
}

#[test]
fn g1_closed_when_accepted_between() {
    let case = Case {
        prior: vec![prior(28, Some("corrected")), prior(29, Some("accepted"))],
        ..Case::new("ok")
    };
    assert_eq!(
        case.check(Question::Relapse),
        (false, "no_recent_correction".to_owned())
    );
    let pending = Case {
        prior: vec![prior(29, None)],
        ..Case::new("ok")
    };
    assert_eq!(pending.check(Question::Relapse).1, "reaction_pending");
}

#[test]
fn relapse_anchor_is_the_correcting_turn() {
    let phrase = phrase_gates("ok", "");
    let two_back = [prior(28, Some("corrected")), prior(29, Some("redirected"))];
    let ctx = GateContext {
        phrase: &phrase,
        trigger: Trigger::TurnEnd,
        facts: None,
        prior: &two_back,
        open_turn_id: 30,
        turn_judge_live: true,
        last_tool: None,
        directives: &[],
        idle_seconds: None,
        quiescence_after_secs: 90,
        ending: "ok",
    };
    assert_eq!(relapse_gate(&ctx).relapse_anchor, Some(29));
    let last = [prior(29, Some("corrected"))];
    let ctx = GateContext {
        prior: &last,
        ..ctx
    };
    assert_eq!(relapse_gate(&ctx).relapse_anchor, Some(30));
}

#[test]
fn g3_opens_only_on_authorised_non_boundary_step() {
    let allow = DirectiveInForce {
        id: 3,
        class: "commit".to_owned(),
        polarity: "allow".to_owned(),
        text: "just commit when green".to_owned(),
        operator_turn_id: Some(2),
    };
    let authorised = Case {
        directives: vec![allow.clone()],
        ..Case::new("Tests pass. Shall I commit this?")
    };
    assert!(authorised.check(Question::AuthorisedHandback).0);
    let unauthorised = Case::new("Tests pass. Shall I commit this?");
    assert_eq!(
        unauthorised.check(Question::AuthorisedHandback).1,
        "not_authorised"
    );
    let boundary = Case {
        directives: vec![allow],
        ..Case::new("Should I force-push the branch?")
    };
    assert!(
        boundary
            .check(Question::AuthorisedHandback)
            .1
            .starts_with("real_decision")
    );
}
