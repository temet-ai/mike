use super::gates::pattern_gate_admits;
use crate::steerer::fleet::context::EvalInputs;
use crate::steerer::registry::{Steerer, SteererParams};

const PROBE_TEXTS: [&str; 40] = [
    "Done. All tests pass.",
    "The change is complete and verified.",
    "I've finished the refactor.",
    "Shipped: the fix is merged.",
    "Everything is green now.",
    "Implemented the retry and deployed it.",
    "Fixed the bug; ready for review.",
    "Resolved the conflict and landed the branch.",
    "I'll report back when the build finishes.",
    "Waiting for the agents to return.",
    "The suite is still running in the background.",
    "I'll check back once it lands.",
    "Standing by for the result.",
    "Next, I'll write the docs.",
    "Let me run the tests again.",
    "Want me to push this?",
    "Shall I open a PR?",
    "Should I merge it now?",
    "Your call on which option.",
    "Let me know if you'd like the other approach.",
    "Bash: grep -rn pattern src",
    "Bash: sleep 30",
    "Bash: cargo test",
    "Grep: fn resolve",
    "Read: src/lib.rs",
    "Edit: src/store/mod.rs",
    "Bash: git commit -m fix",
    "Bash: git push origin main",
    "ok",
    "thanks",
    "looks good",
    "yes, go ahead",
    "that works",
    "no, that's wrong",
    "actually, use the other table",
    "The migration failed with a lock timeout.",
    "Two tests fail: roundtrip and timeout.",
    "I could not reproduce the error.",
    "The proxy is healthy and the queue is empty.",
    "",
];

fn probe_steerer(pattern: &str, kind: &str) -> Steerer {
    Steerer {
        id: 0,
        uuid: uuid::Uuid::nil(),
        name: "gate-probe".to_owned(),
        display_name: "gate probe".to_owned(),
        description: String::new(),
        agent_id: 0,
        authority: "advisory".to_owned(),
        mode: "observe".to_owned(),
        model: String::new(),
        priority: 0,
        enabled: true,
        params: SteererParams::default(),
        config: serde_json::json!({}),
        prompt: None,
        kind: kind.to_owned(),
        trigger_point: "turn_closed".to_owned(),
        invocation: "pattern".to_owned(),
        pattern: Some(pattern.to_owned()),
        auto_enrol: false,
        project_id: None,
        project_path: None,
        created_at: chrono::DateTime::<chrono::Utc>::UNIX_EPOCH,
        updated_at: chrono::DateTime::<chrono::Utc>::UNIX_EPOCH,
    }
}

pub(crate) fn steerer_gate_probe(pattern: &str, kind: &str) -> String {
    let steerer = probe_steerer(pattern, kind);
    let mut bits = String::with_capacity(PROBE_TEXTS.len() * 2);
    for text in PROBE_TEXTS {
        let as_text = EvalInputs {
            eval_text: text.to_owned(),
            ..Default::default()
        };
        let as_tools = EvalInputs {
            tool_calls_summary: text.to_owned(),
            ..Default::default()
        };
        for inputs in [&as_text, &as_tools] {
            bits.push(if pattern_gate_admits(&steerer, inputs) {
                '1'
            } else {
                '0'
            });
        }
    }
    bits
}
