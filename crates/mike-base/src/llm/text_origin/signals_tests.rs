use serde_json::{Value, json};

use super::signals::{Signals, current_text, signals};
use super::state::{Unmatched, state};

const SWEEP: &str = "Wright sweep on infra (session wright-watch). Run the sweep script.";

fn body(system: Value, messages: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({ "system": system, "messages": messages })).unwrap_or_default()
}

fn sweep_session() -> Vec<u8> {
    body(
        json!([{"type": "text", "text": "x-anthropic-billing-header: cc_version=2; cc_entrypoint=cli;"}]),
        json!([
            {"role": "user", "content": "watch infra"},
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "CronCreate",
                 "input": {"cron": "13,38 * * * *", "prompt": SWEEP}}
            ]},
            {"role": "user", "content": SWEEP},
            {"role": "assistant", "content": "No change since the last sweep."},
            {"role": "user", "content": SWEEP}
        ]),
    )
}

#[test]
fn signals_of_a_cron_prompt_count_its_earlier_copies_and_name_the_scheduler() {
    let found = signals(&sweep_session(), SWEEP);
    assert_eq!(
        found,
        Signals {
            sent_before: 1,
            scheduled_by: Some("CronCreate".to_owned()),
            entrypoint: Some("cli".to_owned()),
        }
    );
}

#[test]
fn signals_of_a_new_typed_message_find_no_repeat_or_scheduler() {
    let typed = body(
        json!("x-anthropic-billing-header: cc_entrypoint=sdk-cli;"),
        json!([
            {"role": "user", "content": "review the memory"},
            {"role": "assistant", "content": "ok"},
            {"role": "user", "content": "now fix the splitter"}
        ]),
    );
    let found = signals(&typed, "now fix the splitter");
    assert_eq!(found.sent_before, 0);
    assert_eq!(found.scheduled_by, None);
    assert_eq!(found.entrypoint.as_deref(), Some("sdk-cli"));
}

#[test]
fn signals_of_a_bad_body_are_empty() {
    assert_eq!(signals(b"not json", "hi"), Signals::default());
}

#[test]
fn current_text_is_the_last_user_message() {
    assert_eq!(current_text(&sweep_session()), SWEEP);
}

#[test]
fn state_carries_the_facts_in_words() {
    let input = Unmatched {
        request_id: 1,
        session_db_id: 2,
        text: SWEEP.to_owned(),
        last_user_message: SWEEP.to_owned(),
        previous_user_message: SWEEP.to_owned(),
        previous_assistant_reply: "No change.".to_owned(),
        author: "operator".to_owned(),
        has_agent_id: false,
        signals: signals(&sweep_session(), SWEEP),
    };
    let facts = &state(&input)["facts"];
    assert_eq!(
        facts["sent_before"],
        json!("one earlier user message in this session has exactly this text")
    );
    assert_eq!(
        facts["scheduled_prompt"],
        json!("this session set up CronCreate with exactly this text as its prompt")
    );
    assert_eq!(
        facts["client"],
        json!("an interactive Claude Code session a person can type into")
    );
}
