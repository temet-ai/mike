use super::content_blocks::transform_body;
use super::content_blocks_test_support::{body_bytes, cfg, rule, rule_with_id};
use super::{FilterAction, FilterKind, FilterOutcome};
use serde_json::{Value, json};

fn top_level_spans(body: &[u8]) -> anyhow::Result<std::collections::HashMap<String, String>> {
    let fields: std::collections::HashMap<&str, &serde_json::value::RawValue> =
        serde_json::from_slice(body)?;
    Ok(fields
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.get().to_string()))
        .collect())
}

#[test]
fn rewrite_leaves_untouched_top_level_fields_byte_identical() -> anyhow::Result<()> {
    let raw: &[u8] = br#"{"model":"m","system":[{"type":"text","text":"preamble. anchor phrase here. more text follows."}],"tools":[{"name":"Bash","description":"runs a shell command","input_schema":{"type":"object"}}],"messages":[{"role":"user","content":"hi"}]}"#;
    let c = cfg(vec![rule(
        "append",
        FilterKind::TextSubstring,
        "anchor phrase here",
        FilterAction::Append,
        Some(" EXTRA"),
    )]);

    let FilterOutcome::Rewrote { new_body, .. } = transform_body(raw, &c, "") else {
        anyhow::bail!("expected Rewrote — the anchor is present and the rule is enabled");
    };

    let before = top_level_spans(raw)?;
    let after = top_level_spans(&new_body)?;
    for (key, before_bytes) in &before {
        if key == "system" {
            continue;
        }
        let after_bytes = after
            .get(key)
            .ok_or_else(|| anyhow::anyhow!("key `{key}` missing from the rewritten body"))?;
        assert_eq!(
            before_bytes, after_bytes,
            "key `{key}` must stay byte-identical when only `system` changed \
             (a whole-body reserialize would resort `tools[0]`'s keys and make \
             this fail — see issue #166)"
        );
    }

    let v: Value = serde_json::from_slice(&new_body)?;
    assert_eq!(
        v["system"][0]["text"],
        "preamble. anchor phrase here EXTRA. more text follows."
    );
    Ok(())
}

#[test]
fn envelope_patch_captures_a_system_only_rewrite_alongside_a_tools_array() -> anyhow::Result<()> {
    let raw: &[u8] = br#"{"model":"m","system":[{"type":"text","text":"preamble. anchor phrase here. more text follows."}],"tools":[{"name":"Bash","description":"runs a shell command","input_schema":{"type":"object"}}],"messages":[{"role":"user","content":"hi"}]}"#;
    let c = cfg(vec![rule(
        "append",
        FilterKind::TextSubstring,
        "anchor phrase here",
        FilterAction::Append,
        Some(" EXTRA"),
    )]);

    let FilterOutcome::Rewrote { new_body, .. } = transform_body(raw, &c, "") else {
        anyhow::bail!("expected Rewrote — the anchor is present and the rule is enabled");
    };

    let patch = crate::store::persistence::messages::compute_envelope_patch(raw, &new_body)
        .map_err(|e| anyhow::anyhow!("compute_envelope_patch failed: {e}"))?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "compute_envelope_patch returned None — the filter's system[] rewrite \
                 was not captured for request_sent reconstruction (issue #166)"
            )
        })?;

    let mut reconstructed = raw[..patch.start as usize].to_vec();
    reconstructed.extend_from_slice(&patch.bytes);
    reconstructed.extend_from_slice(&raw[patch.end as usize..]);
    assert_eq!(
        reconstructed, new_body,
        "request_body + the captured envelope patch must reconstruct exactly \
         what the filter forwarded upstream"
    );
    Ok(())
}

#[test]
fn issue_166_rule_71_anchor_in_the_identity_block_never_fires() -> anyhow::Result<()> {
    let body = body_bytes(json!({
        "model": "claude-opus-5",
        "system": [
            {"type": "text", "text": "cch=aaa cc_prev_req=bbb"},
            {"type": "text", "text": "You are Claude Code, Anthropic's official CLI for Claude."},
            {
                "type": "text",
                "text": "main prompt body",
                "cache_control": {"type": "ephemeral", "scope": "global", "ttl": "1h"},
            },
            {
                "type": "text",
                "text": "project guidance in the mutable tail",
                "cache_control": {"type": "ephemeral", "ttl": "1h"},
            },
        ],
        "messages": [{"role": "user", "content": "hi"}],
    }))?;
    let c = cfg(vec![rule_with_id(
        71,
        FilterKind::TextSubstring,
        "You are Claude Code, Anthropic's official CLI for Claude.",
        FilterAction::Append,
        Some("\n\n# Communicating with the user\n..."),
    )]);

    match transform_body(&body, &c, "") {
        FilterOutcome::Unchanged => Ok(()),
        FilterOutcome::Rewrote {
            firing_rule_ids, ..
        } => Err(anyhow::anyhow!(
            "expected Unchanged — the anchor sits in system[1], before the last \
             globally-scoped block at system[2], so the rule must never reach \
             apply_rules_to_text; got a rewrite with firing_rule_ids={firing_rule_ids:?}"
        )),
    }
}

#[test]
fn rewrite_keeps_unmatched_system_blocks_byte_identical_with_their_key_order() -> anyhow::Result<()>
{
    let first = r#"{"type":"text","text":"You are Claude Code."}"#;
    let raw = format!(
        r#"{{"model":"m","system":[{first},{{"type":"text","text":"keep. anchor phrase here. end."}}],"messages":[{{"role":"user","content":"hi"}}]}}"#
    );
    let c = cfg(vec![rule(
        "append",
        FilterKind::TextSubstring,
        "anchor phrase here",
        FilterAction::Append,
        Some(" EXTRA"),
    )]);

    let FilterOutcome::Rewrote { new_body, .. } = transform_body(raw.as_bytes(), &c, "") else {
        anyhow::bail!("expected Rewrote — the anchor is in system[1]");
    };

    let after = std::str::from_utf8(&new_body)?;
    assert!(
        after.contains(&format!(r#""system":[{first},"#)),
        "system[0] matched no rule, so its bytes and key order must survive the rewrite: {after}"
    );
    let rewritten: Value = serde_json::from_slice(&new_body)?;
    assert!(
        rewritten["system"][1]["text"]
            .as_str()
            .is_some_and(|t| t.contains("EXTRA"))
    );
    Ok(())
}

#[test]
fn appended_items_leave_the_earlier_items_byte_identical() -> anyhow::Result<()> {
    let raw =
        br#"{"system":[{"type":"text","text":"a"},{"type":"text","text":"b"}],"messages":[]}"#;
    let value = json!({
        "system": [
            {"type": "text", "text": "a"},
            {"type": "text", "text": "B"},
            {"type": "text", "text": "c"},
        ],
        "messages": [],
    });
    let Some(out) = super::content_blocks::splice_top_level_keys(raw, &value, &["system"]) else {
        anyhow::bail!("the system array must splice");
    };
    let text = std::str::from_utf8(&out)?;
    assert!(
        text.starts_with(r#"{"system":[{"type":"text","text":"a"},"#),
        "an unchanged item keeps its bytes and key order: {text}"
    );
    let back: Value = serde_json::from_slice(&out)?;
    assert_eq!(back, value);
    Ok(())
}
