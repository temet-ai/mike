use anyhow::{Context, bail};
use serde_json::{Value, json};

use super::SystemReplay;

const HOUR: &str = r#"{"type":"ephemeral","ttl":"1h"}"#;
const HOUR_GLOBAL: &str = r#"{"type":"ephemeral","ttl":"1h","scope":"global"}"#;
const FIVE_MINUTES: &str = r#"{"type":"ephemeral"}"#;
const FIVE_MINUTES_GLOBAL: &str = r#"{"type":"ephemeral","scope":"global"}"#;
const PROJECT: &str = "When you use a pronoun for someone, use they/them.";
const FILTERED: &str = "# Session-specific guidance";

fn request(billing: &str, global: &str, block: &str) -> Vec<u8> {
    format!(
        r#"{{"model":"m","messages":[{{"role":"user","content":"hi"}}],"system":[{{"type":"text","text":"x-anthropic-billing-header: cc_version=2.1.330.40f; cch={billing};"}},{{"type":"text","text":"You are Claude Code."}},{{"type":"text","text":"harness","cache_control":{global}}},{block}]}}"#
    )
    .into_bytes()
}

fn as_sent(text: &str, control: &str) -> String {
    format!(r#"{{"type":"text","text":"{text}","cache_control":{control}}}"#)
}

fn as_filtered(text: &str, control_sorted: &str) -> String {
    format!(r#"{{"cache_control":{control_sorted},"text":"{text}","type":"text"}}"#)
}

fn main_thread(billing: &str, text: &str) -> Vec<u8> {
    request(billing, HOUR_GLOBAL, &as_sent(text, HOUR))
}

fn compaction(billing: &str, text: &str) -> Vec<u8> {
    request(billing, FIVE_MINUTES_GLOBAL, &as_sent(text, FIVE_MINUTES))
}

fn filtered_main_thread(billing: &str) -> Vec<u8> {
    let sorted = r#"{"ttl":"1h","type":"ephemeral"}"#;
    request(billing, HOUR_GLOBAL, &as_filtered(FILTERED, sorted))
}

fn filtered_compaction(billing: &str) -> Vec<u8> {
    request(
        billing,
        FIVE_MINUTES_GLOBAL,
        &as_filtered(FILTERED, FIVE_MINUTES),
    )
}

fn system(body: &[u8]) -> anyhow::Result<Vec<Value>> {
    let value: Value = serde_json::from_slice(body).context("replayed body is JSON")?;
    let Some(Value::Array(blocks)) = value.get("system").cloned() else {
        bail!("replayed body has no system array");
    };
    Ok(blocks)
}

fn without_controls(blocks: &[Value]) -> Vec<Value> {
    blocks
        .iter()
        .skip(1)
        .map(|b| {
            let mut b = b.clone();
            b.as_object_mut().map(|o| o.remove("cache_control"));
            b
        })
        .collect()
}

#[test]
fn apply_compaction_whose_system_differs_only_in_cache_ttl_sends_the_rewrite() -> anyhow::Result<()>
{
    let captured_from = main_thread("6fd11", PROJECT);
    let replay = SystemReplay::capture(&captured_from, &filtered_main_thread("6fd11"), &[62], 7)
        .context("capture from the main-thread request")?;

    let compacting = compaction("160ad", PROJECT);
    let Some(sent) = replay.apply(&compacting) else {
        bail!(
            "the compaction request declined the replay, so it would carry the unfiltered prompt"
        );
    };

    let expected = filtered_compaction("160ad");
    assert_eq!(
        String::from_utf8_lossy(&sent),
        String::from_utf8_lossy(&expected),
        "the rewrite lands with the compaction request's own cache_control"
    );
    let main_sent = system(&filtered_main_thread("6fd11"))?;
    assert_eq!(
        without_controls(&system(&sent)?),
        without_controls(&main_sent),
        "the prompt text the cache keys on matches the main thread's"
    );
    Ok(())
}

#[test]
fn apply_main_thread_after_capture_from_a_compaction_keeps_its_hour_ttl() -> anyhow::Result<()> {
    let captured_from = compaction("d89a2", PROJECT);
    let replay = SystemReplay::capture(&captured_from, &filtered_compaction("d89a2"), &[62], 7)
        .context("capture from the compaction request after a restart")?;

    let Some(sent) = replay.apply(&main_thread("c2384", PROJECT)) else {
        bail!("the main-thread request declined the replay");
    };

    let blocks = system(&sent)?;
    let Some(last) = blocks.last() else {
        bail!("no system blocks");
    };
    assert_eq!(last["text"], json!(FILTERED));
    assert_eq!(
        last["cache_control"],
        json!({"type": "ephemeral", "ttl": "1h"})
    );
    assert_eq!(
        blocks.get(2).map(|b| b["cache_control"].clone()),
        Some(json!({"type": "ephemeral", "ttl": "1h", "scope": "global"}))
    );
    Ok(())
}

#[test]
fn apply_compaction_whose_prompt_text_changed_declines() -> anyhow::Result<()> {
    let replay = SystemReplay::capture(
        &main_thread("6fd11", PROJECT),
        &filtered_main_thread("6fd11"),
        &[62],
        7,
    )
    .context("capture")?;
    assert!(
        replay
            .apply(&compaction("160ad", "a different project block"))
            .is_none()
    );
    Ok(())
}

#[test]
fn apply_request_whose_block_lost_its_cache_control_declines() -> anyhow::Result<()> {
    let replay = SystemReplay::capture(
        &main_thread("6fd11", PROJECT),
        &filtered_main_thread("6fd11"),
        &[62],
        7,
    )
    .context("capture")?;
    let bare = request(
        "160ad",
        HOUR_GLOBAL,
        &format!(r#"{{"type":"text","text":"{PROJECT}"}}"#),
    );
    assert!(replay.apply(&bare).is_none());
    Ok(())
}
