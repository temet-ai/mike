use crate::analysis::judge::build_judge_user_message;
use crate::judge_worker::session_poll_digest::build_digest;
use crate::store::persistence::JudgeCandidateRow;
use crate::store::persistence::judge::JudgeDigestRow;

use super::sha256_hex;

const SEPARATOR: &str = "\u{1e}";

pub fn probe_sha256(outputs: &[String]) -> String {
    sha256_hex(outputs.join(SEPARATOR).as_bytes())
}

fn candidate(
    turn_id: i64,
    prompt: String,
    text: String,
    calls: i32,
    errors: i32,
) -> JudgeCandidateRow {
    JudgeCandidateRow {
        turn_id,
        user_prompt: prompt,
        final_assistant_text: text,
        tool_call_count: calls,
        tool_error_count: errors,
    }
}

pub fn turn_judge_candidates() -> Vec<JudgeCandidateRow> {
    vec![
        candidate(
            1,
            "add a retry to the http client".to_owned(),
            "Added the retry with backoff; the tests pass.".to_owned(),
            4,
            0,
        ),
        candidate(
            2,
            "rework the importer so it streams. ".repeat(180),
            "Streaming importer landed. Every step ran the suite. ".repeat(230),
            40,
            3,
        ),
        candidate(3, "and the docs?".to_owned(), String::new(), 0, 0),
    ]
}

pub fn turn_judge_probe() -> String {
    let outputs: Vec<String> = turn_judge_candidates()
        .iter()
        .map(build_judge_user_message)
        .collect();
    probe_sha256(&outputs)
}

pub fn session_digest_turns() -> Vec<JudgeDigestRow> {
    (0..40)
        .map(|i| JudgeDigestRow {
            request_id: 1_000 + i,
            user_prompt: format!("step {i}: continue the migration"),
            stop_response: Some(format!("Finished step {i}. ").repeat(20)),
        })
        .collect()
}

pub fn session_judge_probe() -> String {
    let turns = session_digest_turns();
    let outputs = vec![
        build_digest(&turns, 12, 400),
        build_digest(&turns[..3], 12, 0),
        build_digest(&[], 12, 400),
    ];
    probe_sha256(&outputs)
}

pub fn steerer_probe(kind: &str, trigger_point: &str, verifier_budget: &str) -> String {
    let output = crate::steerer::fleet::steerer_input_probe(kind, trigger_point, verifier_budget);
    sha256_hex(output.as_bytes())
}

pub fn gate_probe(pattern: &str, kind: &str) -> String {
    sha256_hex(crate::steerer::fleet::steerer_gate_probe(pattern, kind).as_bytes())
}
