use super::digest::*;
use crate::store::persistence::judge::JudgeDigestRow;

#[test]
fn judge_verdict_deserializes_from_json() -> anyhow::Result<()> {
    let json = r#"{
        "outcome": "success",
        "quality_score": 0.85,
        "rationale": "The session completed its task cleanly.",
        "overview": "The operator asked for a bug fix.\n\nThe session fixed it and added tests."
    }"#;
    let verdict: JudgeVerdict = serde_json::from_str(json)?;
    assert_eq!(verdict.outcome, "success");
    assert!((verdict.quality_score - 0.85).abs() < 0.001);
    assert_eq!(verdict.rationale, "The session completed its task cleanly.");
    assert_eq!(
        verdict.overview,
        "The operator asked for a bug fix.\n\nThe session fixed it and added tests."
    );
    Ok(())
}

#[test]
fn judge_verdict_deserializes_empty_overview() -> anyhow::Result<()> {
    let json = r#"{
        "outcome": "unknown",
        "quality_score": 0.0,
        "rationale": "There were no operator turns in this session.",
        "overview": ""
    }"#;
    let verdict: JudgeVerdict = serde_json::from_str(json)?;
    assert_eq!(verdict.overview, "");
    Ok(())
}

#[test]
fn outcome_code_mapping_is_correct() {
    assert_eq!(outcome_code_to_id("success"), 2);
    assert_eq!(outcome_code_to_id("partial"), 3);
    assert_eq!(outcome_code_to_id("failure"), 4);
    assert_eq!(outcome_code_to_id("unknown"), 1);
    assert_eq!(outcome_code_to_id("garbage"), 1);
}

#[test]
fn build_digest_empty_returns_message() {
    let digest = build_digest(&[], 40, 0);
    assert!(digest.contains("No operator turns"));
}

#[test]
fn build_digest_formats_turns() {
    let turns = vec![
        JudgeDigestRow {
            request_id: 1,
            user_prompt: "Fix the bug".to_string(),
            stop_response: Some("Done.".to_string()),
        },
        JudgeDigestRow {
            request_id: 2,
            user_prompt: "Add a test".to_string(),
            stop_response: None,
        },
    ];
    let digest = build_digest(&turns, 40, 0);
    assert!(digest.contains("Turn 1"));
    assert!(digest.contains("Fix the bug"));
    assert!(digest.contains("Done."));
    assert!(digest.contains("Turn 2"));
    assert!(digest.contains("Add a test"));
}

#[test]
fn build_digest_caps_each_turns_response_from_the_end() {
    let turns = vec![JudgeDigestRow {
        request_id: 1,
        user_prompt: "Fix the bug".to_string(),
        stop_response: Some(
            "OPENING-NARRATION filler filler filler CLOSING-DELIVERY: fixed.".to_string(),
        ),
    }];
    let digest = build_digest(&turns, 40, 30);
    assert!(
        digest.contains("CLOSING-DELIVERY: fixed."),
        "the delivery must survive truncation, got: {digest}"
    );
    assert!(
        !digest.contains("OPENING-NARRATION"),
        "the opening narration is what should be cut, got: {digest}"
    );
}

#[test]
fn build_digest_zero_max_chars_per_turn_means_no_cap() {
    let long_response = "x".repeat(10_000);
    let turns = vec![JudgeDigestRow {
        request_id: 1,
        user_prompt: "Fix the bug".to_string(),
        stop_response: Some(long_response.clone()),
    }];
    let digest = build_digest(&turns, 40, 0);
    assert!(digest.contains(&long_response));
}

#[test]
fn build_digest_caps_retains_first_turn_and_marks_omitted() {
    let turns: Vec<JudgeDigestRow> = (1..=10)
        .map(|i| JudgeDigestRow {
            request_id: i,
            user_prompt: format!("prompt {i}"),
            stop_response: None,
        })
        .collect();
    let digest = build_digest(&turns, 3, 0);
    assert!(
        digest.contains("request_id=1)"),
        "the earliest turn must be retained under the cap"
    );
    assert!(
        !digest.contains("request_id=2)"),
        "middle turns must be omitted, not silently kept"
    );
    assert!(
        digest.contains("7 earlier turn(s) omitted"),
        "the omitted count must be named explicitly; got: {digest}"
    );
    assert!(digest.contains("request_id=9)"));
    assert!(digest.contains("request_id=10)"));
}

#[test]
fn build_digest_uncapped_matches_prior_format_bytewise() {
    let turns = vec![
        JudgeDigestRow {
            request_id: 5,
            user_prompt: "  do X  ".to_string(),
            stop_response: Some("  done X  ".to_string()),
        },
        JudgeDigestRow {
            request_id: 6,
            user_prompt: "do Y".to_string(),
            stop_response: None,
        },
    ];
    let digest = build_digest(&turns, 40, 0);
    let expected = "SESSION TURN DIGEST\n\
===================\n\n\
--- Turn 1 (request_id=5) ---\n\
USER PROMPT:\n\
do X\n\
\n\
ASSISTANT RESPONSE:\n\
done X\n\
\n\
--- Turn 2 (request_id=6) ---\n\
USER PROMPT:\n\
do Y\n\
\n";
    assert_eq!(digest, expected);
}

#[test]
fn judge_verdict_schema_is_valid_json() {
    let schema = judge_verdict_schema();
    assert!(schema.is_object());
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"].is_object());
    assert!(schema["properties"]["outcome"].is_object());
    assert!(schema["properties"]["quality_score"].is_object());
    assert!(schema["properties"]["rationale"].is_object());
    assert!(schema["properties"]["overview"].is_object());
}

#[test]
fn judge_verdict_schema_requires_overview() {
    let schema = judge_verdict_schema();
    let required: Vec<&str> = schema["required"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(
        required.contains(&"overview"),
        "overview must be a required field; got: {required:?}"
    );
}

#[test]
fn judge_verdict_schema_requires_topics_and_learnings() {
    let schema = judge_verdict_schema();
    assert!(schema["properties"]["topics"].is_object());
    assert!(schema["properties"]["learnings"].is_object());
    let required: Vec<&str> = schema["required"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(
        required.contains(&"topics") && required.contains(&"learnings"),
        "topics and learnings must be required fields; got: {required:?}"
    );
}

#[test]
fn judge_verdict_deserializes_topics_and_learnings() -> anyhow::Result<()> {
    let json = r#"{
        "outcome": "success",
        "quality_score": 0.85,
        "rationale": "Shipped cleanly.",
        "overview": "Goal.\n\nOutcome.",
        "topics": ["session judge", "embeddings"],
        "learnings": ["the digest must include related sessions"]
    }"#;
    let verdict: JudgeVerdict = serde_json::from_str(json)?;
    assert_eq!(verdict.topics, vec!["session judge", "embeddings"]);
    assert_eq!(
        verdict.learnings,
        vec!["the digest must include related sessions"]
    );
    Ok(())
}

#[test]
fn judge_verdict_defaults_topics_and_learnings_when_absent() -> anyhow::Result<()> {
    let json = r#"{
        "outcome": "success",
        "quality_score": 0.85,
        "rationale": "Shipped cleanly.",
        "overview": "Goal.\n\nOutcome."
    }"#;
    let verdict: JudgeVerdict = serde_json::from_str(json)?;
    assert!(verdict.topics.is_empty());
    assert!(verdict.learnings.is_empty());
    Ok(())
}

#[test]
fn build_related_sessions_block_empty_when_no_neighbours() {
    assert_eq!(build_related_sessions_block(&[]), "");
}

#[test]
fn build_related_sessions_block_includes_title_and_learning() -> anyhow::Result<()> {
    let related = vec![RelatedSessionContext {
        title: Some("Fix the mailbox stranding bug".to_string()),
        last_seen: chrono::DateTime::parse_from_rfc3339("2026-08-30T12:00:00Z")?
            .with_timezone(&chrono::Utc),
        outcome: Some("success".to_string()),
        similarity: 0.8123,
        overview: Some("Goal.\n\nOutcome.".to_string()),
        learnings: vec!["mailbox delivery must ack before the sender clears".to_string()],
    }];
    let block = build_related_sessions_block(&related);
    assert!(
        block.contains("Fix the mailbox stranding bug"),
        "neighbour title must appear in the block; got: {block}"
    );
    assert!(
        block.contains("mailbox delivery must ack before the sender clears"),
        "neighbour learning must appear in the block; got: {block}"
    );
    assert!(
        block.contains("0.81"),
        "similarity must be formatted to two decimals; got: {block}"
    );
    assert!(
        block.contains("2026-08-30"),
        "last-seen date must appear in the block; got: {block}"
    );
    assert!(
        block
            .to_lowercase()
            .contains("name that session by its title"),
        "the block must instruct the judge to name a continued/repeated/corrected \
         session by title; got: {block}"
    );
    Ok(())
}

#[test]
fn build_related_sessions_block_caps_overview_and_drops_low_similarity() -> anyhow::Result<()> {
    let last_seen =
        chrono::DateTime::parse_from_rfc3339("2026-08-30T12:00:00Z")?.with_timezone(&chrono::Utc);
    let related: Vec<RelatedSessionContext> = (0..5)
        .map(|i| RelatedSessionContext {
            title: Some(format!("neighbour session {i}")),
            last_seen,
            outcome: Some("success".to_string()),
            similarity: 0.9 - (f64::from(i) * 0.1),
            overview: Some("x".repeat(3_000)),
            learnings: vec!["y".repeat(500)],
        })
        .collect();

    let block = build_related_sessions_block(&related);

    assert!(
        block.len() <= RELATED_SESSIONS_BLOCK_MAX_CHARS,
        "block must respect the whole-block cap; got {} chars",
        block.len()
    );
    assert!(
        block.contains("neighbour session 0"),
        "the highest-similarity neighbour must survive dropping; got: {block}"
    );
    assert!(
        !block.contains("neighbour session 4"),
        "the lowest-similarity neighbour must be dropped when the block would overflow; \
         got: {block}"
    );
    for line in block.lines() {
        if let Some(overview) = line.strip_prefix("  Overview: ") {
            assert!(
                overview.chars().count() <= RELATED_SESSION_OVERVIEW_MAX_CHARS,
                "overview must be capped at {RELATED_SESSION_OVERVIEW_MAX_CHARS} chars; \
                 got {} chars",
                overview.chars().count()
            );
        }
        if let Some(learning) = line.strip_prefix("    - ") {
            assert!(
                learning.chars().count() <= RELATED_SESSION_LEARNING_MAX_CHARS,
                "learning must be capped at {RELATED_SESSION_LEARNING_MAX_CHARS} chars; \
                 got {} chars",
                learning.chars().count()
            );
        }
    }
    Ok(())
}
