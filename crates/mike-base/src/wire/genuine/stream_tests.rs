use std::collections::BTreeMap;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;

use super::repeat::repeat_key;
use super::stream::{StreamMessage, TurnResolution, resolve_turns};
use super::{PromptInput, classify_prompt};

const FIXTURE: &str = include_str!("../../../tests/fixtures/dossier/j2_controls.json");

fn parse_time(value: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc))
}

fn load_session(prefix: &str) -> Result<Vec<StreamMessage>> {
    let root: Value = serde_json::from_str(FIXTURE)?;
    let sessions = root["sessions"].as_array().context("sessions array")?;
    let session = sessions
        .iter()
        .find(|s| {
            s["session"]
                .as_str()
                .is_some_and(|id| id.starts_with(prefix))
        })
        .context("session in fixture")?;
    let mut stream = Vec::new();
    for message in session["messages"].as_array().context("messages array")? {
        let text = message["text"].as_str().unwrap_or_default();
        let capture = classify_prompt(&PromptInput {
            text,
            last_user_message: None,
            author: message["author"].as_str().unwrap_or("operator"),
            has_agent_id: message["has_agent_id"].as_bool().unwrap_or(false),
        });
        stream.push(StreamMessage {
            request_id: message["request_id"].as_i64().context("request_id")?,
            at: parse_time(message["created_at"].as_str().context("created_at")?)?,
            capture,
            repeat_key: repeat_key(text),
            turn_id: message["turn_id"].as_i64(),
        });
    }
    Ok(stream)
}

fn as_of() -> Result<DateTime<Utc>> {
    parse_time("2026-09-23T17:22:00+00:00")
}

fn resolution(session: &str, turn_id: i64) -> Result<TurnResolution> {
    let stream = load_session(session)?;
    resolve_turns(&stream, as_of()?)
        .into_iter()
        .find(|r| r.turn_id == turn_id)
        .context("turn resolved")
}

fn skipped(pairs: &[(&'static str, u32)]) -> BTreeMap<&'static str, u32> {
    pairs.iter().copied().collect()
}

fn assert_next(r: &TurnResolution, turn: i64, request: i64) {
    let next = r.next_genuine.as_ref();
    assert_eq!(next.and_then(|n| n.turn_id), Some(turn));
    assert_eq!(next.map(|n| n.request_id), Some(request));
}

#[test]
fn resolve_turns_16368_next_is_request_434799_turn_16370_nothing_skipped() -> Result<()> {
    let r = resolution("1ce3a5c8", 16368)?;
    assert_next(&r, 16370, 434799);
    assert!(r.skipped.is_empty());
    Ok(())
}

#[test]
fn resolve_turns_14959_is_loop_wakeup_rule_a_and_next_is_turn_15075_request_338176_skipping_13_loop_2_peer()
-> Result<()> {
    let r = resolution("1c59286c", 14959)?;
    assert_eq!(r.effective.kind, super::PromptKind::LoopWakeup);
    assert_eq!(r.effective.rule, Some(super::EffectiveRule::A));
    assert_next(&r, 15075, 338176);
    assert_eq!(
        r.skipped,
        skipped(&[("loop-wakeup", 13), ("peer-message", 2)])
    );
    Ok(())
}

#[test]
fn resolve_turns_15395_next_is_burst_head_turn_15398_request_362923_skipping_6_peer() -> Result<()>
{
    let r = resolution("0267a075", 15395)?;
    assert_next(&r, 15398, 362923);
    assert_eq!(r.skipped, skipped(&[("peer-message", 6)]));
    Ok(())
}

#[test]
fn resolve_turns_burst_15398_to_15405_each_genuine_and_points_to_the_next_turn() -> Result<()> {
    let expected = [
        (15398, 15399, 362926),
        (15399, 15400, 362928),
        (15400, 15401, 362932),
        (15401, 15402, 362936),
        (15402, 15403, 362940),
        (15403, 15404, 362946),
        (15404, 15405, 362952),
    ];
    for (turn, next_turn, next_request) in expected {
        let r = resolution("0267a075", turn)?;
        assert_eq!(r.effective.kind, super::PromptKind::Genuine, "turn {turn}");
        assert_next(&r, next_turn, next_request);
        assert!(r.skipped.is_empty(), "turn {turn}");
    }
    Ok(())
}

#[test]
fn resolve_turns_15405_next_is_turn_15408_request_370695() -> Result<()> {
    let r = resolution("0267a075", 15405)?;
    assert_next(&r, 15408, 370695);
    Ok(())
}

#[test]
fn resolve_turns_14753_has_no_next_and_skips_8_loop_2_hook() -> Result<()> {
    let r = resolution("7224c82c", 14753)?;
    assert!(r.next_genuine.is_none());
    assert_eq!(
        r.skipped,
        skipped(&[("loop-wakeup", 8), ("hook-feedback", 2)])
    );
    Ok(())
}

#[test]
fn resolve_turns_15614_has_no_next_and_skips_4_loop_12_peer_2_hook() -> Result<()> {
    let r = resolution("d34c25a1", 15614)?;
    assert!(r.next_genuine.is_none());
    assert_eq!(
        r.skipped,
        skipped(&[
            ("loop-wakeup", 4),
            ("peer-message", 12),
            ("hook-feedback", 2)
        ])
    );
    Ok(())
}

#[test]
fn resolve_turns_hides_messages_after_as_of() -> Result<()> {
    let stream = load_session("1ce3a5c8")?;
    let first = stream.first().context("first message")?.at;
    let resolved = resolve_turns(&stream, first);
    assert!(
        resolved
            .iter()
            .all(|r| r.request_id == stream[0].request_id)
    );
    Ok(())
}

#[test]
fn resolve_turns_lists_skipped_openers_as_machine_segments() -> Result<()> {
    let r = resolution("1c59286c", 14959)?;
    let stream = load_session("1c59286c")?;
    let openers = stream
        .iter()
        .filter(|m| m.request_id > r.request_id)
        .take_while(|m| Some(m.request_id) != r.next_genuine.as_ref().map(|n| n.request_id))
        .filter_map(|m| m.turn_id)
        .collect::<Vec<_>>();
    assert_eq!(r.machine_segment_ids, openers);
    Ok(())
}

const BURST_PREFIX: &str = "Please for the love of all. its now 15 hours.";
const MAX_MACHINE_TEXT_CHARS: usize = 300;

#[test]
fn fixture_keeps_only_placeholders_for_genuine_and_short_machine_text() -> Result<()> {
    let root: Value = serde_json::from_str(FIXTURE)?;
    for session in root["sessions"].as_array().context("sessions array")? {
        for message in session["messages"].as_array().context("messages array")? {
            let text = message["text"].as_str().unwrap_or_default();
            let request_id = message["request_id"].as_i64().context("request_id")?;
            let class = classify_prompt(&PromptInput {
                text,
                last_user_message: None,
                author: message["author"].as_str().unwrap_or("operator"),
                has_agent_id: message["has_agent_id"].as_bool().unwrap_or(false),
            });
            if class.kind == super::PromptKind::Genuine {
                let placeholder = format!("operator message {request_id}");
                assert!(
                    text == placeholder || text.starts_with(BURST_PREFIX),
                    "request {request_id}: genuine text is not redacted"
                );
            } else {
                assert!(
                    text.chars().count() <= MAX_MACHINE_TEXT_CHARS,
                    "request {request_id}: machine text exceeds {MAX_MACHINE_TEXT_CHARS} chars"
                );
            }
        }
    }
    Ok(())
}

const REPO_PATH_PREFIX: &str = "/workdir/projects/temet/temet-proxy";
const OTHER_PATH: &str = "/workdir/other";

fn disallowed_paths(text: &str) -> Vec<String> {
    let pattern = match regex::Regex::new(r"/[A-Za-z0-9_.-]+(?:/[A-Za-z0-9_.-]+)+") {
        Ok(pattern) => pattern,
        Err(e) => panic!("invalid path pattern: {e}"),
    };
    pattern
        .find_iter(text)
        .map(|found| found.as_str().trim_end_matches(['.', ',']).to_string())
        .filter(|path| {
            let in_repo =
                path == REPO_PATH_PREFIX || path.starts_with(&format!("{REPO_PATH_PREFIX}/"));
            !(in_repo || path == OTHER_PATH)
        })
        .collect()
}

#[test]
fn fixture_paths_are_workdir_repo_paths_or_workdir_other() -> Result<()> {
    let root: Value = serde_json::from_str(FIXTURE)?;
    for session in root["sessions"].as_array().context("sessions array")? {
        for message in session["messages"].as_array().context("messages array")? {
            let text = message["text"].as_str().unwrap_or_default();
            let bad = disallowed_paths(text);
            assert!(
                bad.is_empty(),
                "request {}: path outside the allowlist: {bad:?}",
                message["request_id"]
            );
        }
    }
    Ok(())
}

#[test]
fn path_guard_catches_the_truncated_client_path_of_request_330052() {
    let old_text = "Fleet caretaker-alive coordinator cycle. Worklog /workdir/projects/work/wai";
    assert_eq!(
        disallowed_paths(old_text),
        vec!["/workdir/projects/work/wai".to_string()]
    );
    assert!(
        disallowed_paths(
            "Worklog /workdir/other and /workdir/projects/temet/temet-proxy/docs/a.md."
        )
        .is_empty()
    );
    assert_eq!(disallowed_paths("see /Users/someone/projects/x").len(), 1);
}

#[test]
fn fixture_contains_no_addresses_or_urls() {
    for needle in ["@", "http://", "https://"] {
        assert!(!FIXTURE.contains(needle), "fixture contains {needle}");
    }
}

const KEPT_LOOP_TOKEN_REQUEST_ID: i64 = 329857;
const KEPT_LOOP_TOKEN_TEXT: &str = "operator message 329857 ScheduleWakeup";

#[test]
fn fixture_request_329857_keeps_only_the_schedule_wakeup_token_so_rule_a_still_fires() -> Result<()>
{
    let root: Value = serde_json::from_str(FIXTURE)?;
    let mut found = 0;
    for session in root["sessions"].as_array().context("sessions array")? {
        for message in session["messages"].as_array().context("messages array")? {
            if message["request_id"].as_i64() != Some(KEPT_LOOP_TOKEN_REQUEST_ID) {
                continue;
            }
            found += 1;
            let text = message["text"].as_str().unwrap_or_default();
            assert_eq!(text, KEPT_LOOP_TOKEN_TEXT);
            let class = classify_prompt(&PromptInput {
                text,
                last_user_message: None,
                author: "operator",
                has_agent_id: false,
            });
            assert_eq!(class.kind, super::PromptKind::LoopWakeup);
            assert_eq!(class.rule, Some(super::CaptureRule::A));
        }
    }
    assert_eq!(found, 1);
    Ok(())
}
