use super::facts::{AskSource, Facts, NudgeSource, Sources, ask_source, nudge_source};
use super::rules::{COMPACTION_ASK, EMPTY_ASK, MACHINE_ASK, MAILBOX_NUDGE, OVERRIDE_NUDGE, settle};
use crate::steerer::verdict::{Need, SteererVerdict};

const MAILBOX: &str = "You have an unread cross-session mailbox message (status: pending, unclaimed). Check your mailbox before treating this session as done.";

fn verdict(nudge: &str) -> SteererVerdict {
    SteererVerdict {
        fires: true,
        confidence: 0.9,
        reason: String::new(),
        nudge: nudge.to_owned(),
        need: Need::High,
        checklist: Vec::new(),
    }
}

fn facts(nudge_source: NudgeSource, judged_ask: &str) -> Facts {
    Facts {
        nudge_source,
        judged_ask: judged_ask.to_owned(),
        ask_source: ask_source(judged_ask),
        recent_commits: Vec::new(),
    }
}

#[test]
fn nudge_source_verifier_text_is_verifier() {
    let nudge = "You did not run the tests.";
    assert_eq!(
        nudge_source(nudge, &verdict(nudge), &["deploy to infra and the Mac"]),
        NudgeSource::Verifier
    );
}

#[test]
fn nudge_source_quoting_an_acceptance_criterion_is_criteria() {
    let nudge = "You asked for \"Deploy to infra and the Mac\" and I do not see that in this turn.";
    assert_eq!(
        nudge_source(nudge, &verdict(nudge), &["deploy to infra and the Mac"]),
        NudgeSource::Criteria
    );
}

#[test]
fn nudge_source_mailbox_override_is_mailbox_and_another_text_is_other() {
    assert_eq!(
        nudge_source(MAILBOX, &verdict(""), &[]),
        NudgeSource::Mailbox
    );
    assert_eq!(
        nudge_source("something else", &verdict("the verifier said this"), &[]),
        NudgeSource::Other
    );
}

#[test]
fn ask_source_reads_compaction_peer_mail_and_operator_words() {
    assert_eq!(
        ask_source("[initial] <analysis>\nThe user asked...</analysis>\n<summary>..."),
        AskSource::Compaction
    );
    assert_eq!(
        ask_source("<summary>1. Primary request</summary>"),
        AskSource::Compaction
    );
    assert_eq!(
        ask_source("Another Claude session sent you a message: please rebase"),
        AskSource::PeerMail
    );
    assert_eq!(
        ask_source("[initial] fix the clearance on the cart"),
        AskSource::Operator
    );
    assert_eq!(ask_source("  "), AskSource::Unknown);
    assert_eq!(
        ask_source("Stop hook feedback: run the tests"),
        AskSource::Unknown
    );
}

#[test]
fn facts_derive_judges_the_accumulated_ask_before_the_turn_prompt() {
    let nudge = "You did not run the tests.";
    let v = verdict(nudge);
    let sources = Sources {
        nudge,
        verdict: &v,
        criteria: &[],
        accumulated_ask: "<analysis>summary of earlier work</analysis>",
        operator_ask: "",
        last_user_message: "carry on",
    };
    let derived = Facts::derive(&sources, vec!["7d199613c Fix it".to_owned()]);
    assert_eq!(derived.ask_source, AskSource::Compaction);
    assert_eq!(derived.nudge_source, NudgeSource::Verifier);
    assert_eq!(derived.recent_commits.len(), 1);
}

#[test]
fn settle_holds_override_nudges_and_machine_or_empty_asks() {
    assert_eq!(
        settle(&facts(NudgeSource::Mailbox, "fix it")),
        Some(MAILBOX_NUDGE)
    );
    assert_eq!(
        settle(&facts(NudgeSource::Other, "fix it")),
        Some(OVERRIDE_NUDGE)
    );
    assert_eq!(settle(&facts(NudgeSource::Verifier, "")), Some(EMPTY_ASK));
    assert_eq!(
        settle(&facts(NudgeSource::Verifier, "<summary>x</summary>")),
        Some(COMPACTION_ASK)
    );
    assert_eq!(
        settle(&facts(NudgeSource::Criteria, "Stop hook feedback: x")),
        Some(MACHINE_ASK)
    );
}

#[test]
fn settle_leaves_an_operator_ask_to_jev() {
    assert_eq!(
        settle(&facts(NudgeSource::Verifier, "fix the clearance")),
        None
    );
    assert_eq!(
        settle(&facts(
            NudgeSource::Criteria,
            "Another Claude session sent you: rebase"
        )),
        None
    );
}
