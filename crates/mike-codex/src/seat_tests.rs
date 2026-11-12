use serde_json::json;

use crate::TurnOutcome;
use crate::seat::absorb;

#[test]
fn turn_outcome_collects_final_answer_and_commands_of_its_own_thread() {
    let mut outcome = TurnOutcome {
        turn_id: "u-1".into(),
        ..TurnOutcome::default()
    };
    let other = json!({"method": "turn/completed", "params": {"threadId": "t-2", "turn": {"id": "u-1", "status": "completed"}}});
    assert!(!absorb(&mut outcome, &other, "t-1"));
    let command = json!({"method": "item/completed", "params": {"threadId": "t-1", "item": {"type": "commandExecution", "command": "./check.sh", "exitCode": 0}}});
    let answer = json!({"method": "item/completed", "params": {"threadId": "t-1", "item": {"type": "agentMessage", "phase": "final_answer", "text": "done"}}});
    let done = json!({"method": "turn/completed", "params": {"threadId": "t-1", "turn": {"id": "u-1", "status": "completed"}}});
    assert!(!absorb(&mut outcome, &command, "t-1"));
    assert!(!absorb(&mut outcome, &answer, "t-1"));
    assert!(absorb(&mut outcome, &done, "t-1"));
    assert_eq!(outcome.final_text, "done");
    assert_eq!(outcome.status, "completed");
    assert_eq!(outcome.commands, vec![("./check.sh".to_string(), Some(0))]);
}

#[test]
fn turn_outcome_reports_gaps_and_bounded_command_history() {
    let mut outcome = TurnOutcome {
        turn_id: "turn".into(),
        ..TurnOutcome::default()
    };
    let item = json!({"method":"item/completed","params":{"threadId":"thread","turnId":"turn","item":{
        "type":"commandExecution","command":"x".repeat(4096)
    }}});
    for _ in 0..30 {
        absorb(&mut outcome, &item, "thread");
    }
    assert_eq!(outcome.commands.len(), 16);
    assert!(outcome.commands_truncated);
    absorb(
        &mut outcome,
        &json!({"method":"mike/observationGap"}),
        "thread",
    );
    assert!(outcome.observations_incomplete);
    let unrelated = json!({"method":"item/completed","params":{"threadId":"thread","turnId":"other","item":{
        "type":"agentMessage","phase":"final_answer","text":"other turn"
    }}});
    absorb(&mut outcome, &unrelated, "thread");
    assert!(outcome.final_text.is_empty());
}
