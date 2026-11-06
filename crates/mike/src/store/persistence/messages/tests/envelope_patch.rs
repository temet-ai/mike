use crate::store::persistence::messages::envelope_patch::common_prefix_suffix;
use crate::store::persistence::messages::{
    SentPatch, SentPatchRow, apply_sent_patches, canonicalise_body_messages,
    compute_envelope_patch, compute_sent_patches, whole_body_envelope_patch,
};

#[test]
fn common_prefix_suffix_full_match_reports_whole_prefix_no_suffix() {
    assert_eq!(common_prefix_suffix(b"abcdef", b"abcdef"), (6, 0));
}

#[test]
fn common_prefix_suffix_bounds_insertion_to_a_single_point() {
    assert_eq!(common_prefix_suffix(b"abcdef", b"abcXdef"), (3, 3));
}

#[test]
fn common_prefix_suffix_bounds_replacement_span() {
    assert_eq!(common_prefix_suffix(b"abcXXXdef", b"abcYYdef"), (3, 3));
}

#[test]
fn envelope_patch_tools_append_reconstructs_zero_width_insertions() -> anyhow::Result<()> {
    let cases: [(&[u8], &[u8]); 2] = [
        (
            br#"{"messages":[],"tools":[]}"#,
            br#"{"messages":[],"tools":[{"name":"mike_session_roster"}]}"#,
        ),
        (
            br#"{"messages":[],"tools":[{"name":"Read"}]}"#,
            br#"{"messages":[],"tools":[{"name":"Read"},{"name":"mike_session_roster"}]}"#,
        ),
    ];
    for (original, sent) in cases {
        let patch = compute_envelope_patch(original, sent)?
            .ok_or_else(|| anyhow::anyhow!("tools append must produce an envelope patch"))?;
        assert_eq!(patch.start, patch.end);
        assert!(!patch.bytes.is_empty());
        let reconstructed = apply_sent_patches(
            original,
            vec![SentPatchRow {
                sent_patch_start: patch.start,
                sent_patch_end: patch.end,
                sent_patch_bytes: patch.bytes,
            }],
        )?;
        assert_eq!(reconstructed, sent);
    }
    Ok(())
}

#[test]
fn envelope_patch_empty_source_reconstructs_insertion_at_zero() -> anyhow::Result<()> {
    let sent = br#"{"messages":[],"tools":[]}"#;
    let patch = whole_body_envelope_patch(b"", sent)?
        .ok_or_else(|| anyhow::anyhow!("empty source must produce a whole-body insertion"))?;
    assert_eq!((patch.start, patch.end), (0, 0));
    let reconstructed = apply_sent_patches(
        b"",
        vec![SentPatchRow {
            sent_patch_start: patch.start,
            sent_patch_end: patch.end,
            sent_patch_bytes: patch.bytes,
        }],
    )?;
    assert_eq!(reconstructed, sent);
    Ok(())
}

fn apply_one_patch(original: &[u8], patch: &SentPatch) -> Vec<u8> {
    let mut out = original[..patch.start as usize].to_vec();
    out.extend_from_slice(&patch.bytes);
    out.extend_from_slice(&original[patch.end as usize..]);
    out
}

#[test]
fn compute_envelope_patch_locates_system_block_splice() -> anyhow::Result<()> {
    let request_body =
            br#"{"model":"m","system":[{"type":"text","text":"OLD-BLOCK"}],"messages":[{"role":"user","content":"hi"}]}"#;
    let sent_body =
            br#"{"model":"m","system":[{"type":"text","text":"NEW-BLOCK-LONGER"}],"messages":[{"role":"user","content":"hi"}]}"#;
    let patch = compute_envelope_patch(request_body, sent_body)?
        .ok_or_else(|| anyhow::anyhow!("expected an envelope patch"))?;
    assert_eq!(apply_one_patch(request_body, &patch), sent_body);
    Ok(())
}

#[test]
fn compute_envelope_patch_none_when_only_a_message_changed() -> anyhow::Result<()> {
    let request_body =
            br#"{"model":"m","system":[{"type":"text","text":"SAME"}],"messages":[{"role":"user","content":"hi"}]}"#;
    let sent_body =
            br#"{"model":"m","system":[{"type":"text","text":"SAME"}],"messages":[{"role":"user","content":"bye"}]}"#;
    assert!(
        compute_envelope_patch(request_body, sent_body)?.is_none(),
        "a message-only mutation belongs to compute_sent_patches, not the envelope"
    );
    Ok(())
}

#[test]
fn compute_envelope_patch_none_when_bodies_identical() -> anyhow::Result<()> {
    let body = br#"{"model":"m","messages":[{"role":"user","content":"hi"}]}"#;
    assert!(compute_envelope_patch(body, body)?.is_none());
    Ok(())
}

#[test]
fn compute_envelope_patch_locates_the_changed_field_without_a_messages_key() -> anyhow::Result<()> {
    let request_body = br#"{"model":"m","input":"OLD"}"#;
    let sent_body = br#"{"model":"m","input":"NEW-VALUE"}"#;
    let patch = compute_envelope_patch(request_body, sent_body)?
        .ok_or_else(|| anyhow::anyhow!("expected a patch on the `input` field"))?;
    assert_eq!(apply_one_patch(request_body, &patch), sent_body);
    Ok(())
}

#[test]
fn compute_envelope_patch_falls_back_to_whole_body_on_message_count_mismatch() -> anyhow::Result<()>
{
    let request_body = br#"{"system":"x","messages":[{"role":"user","content":"hi"}]}"#;
    let sent_body = br#"{"system":"x","messages":[{"role":"user","content":"hi"},{"role":"assistant","content":"yo"}]}"#;
    let patch = compute_envelope_patch(request_body, sent_body)?
        .ok_or_else(|| anyhow::anyhow!("expected a whole-body fallback patch"))?;
    assert_eq!(apply_one_patch(request_body, &patch), sent_body);
    Ok(())
}

#[test]
fn compute_envelope_patch_locates_a_field_that_sits_after_messages() -> anyhow::Result<()> {
    let request_body =
        br#"{"system":"x","messages":[{"role":"user","content":"hi"}],"tail":"OLD"}"#;
    let sent_body = br#"{"system":"x","messages":[{"role":"user","content":"hi"}],"tail":"NEW"}"#;
    let patch = compute_envelope_patch(request_body, sent_body)?
        .ok_or_else(|| anyhow::anyhow!("expected a patch on the `tail` field"))?;
    assert_eq!(apply_one_patch(request_body, &patch), sent_body);
    Ok(())
}

#[test]
fn compute_envelope_patch_fails_closed_when_two_fields_change() -> anyhow::Result<()> {
    let request_body =
            br#"{"model":"m","system":"OLD-SYS","messages":[{"role":"user","content":"hi"}],"tail":"OLD-TAIL"}"#;
    let sent_body =
            br#"{"model":"m","system":"NEW-SYS","messages":[{"role":"user","content":"hi"}],"tail":"NEW-TAIL"}"#;
    assert!(compute_envelope_patch(request_body, sent_body)?.is_none());
    Ok(())
}

#[test]
fn compute_envelope_patch_covers_system_and_tools_changing_together() -> anyhow::Result<()> {
    let request_body = br#"{"model":"m","messages":[{"role":"user","content":"hi"}],"system":[{"type":"text","text":"OLD-SYS"}],"tools":[{"name":"Read"}],"max_tokens":16}"#;
    let sent_body = br#"{"model":"m","messages":[{"role":"user","content":"hi"}],"system":[{"type":"text","text":"NEW-SYS-REWRITTEN"}],"tools":[{"name":"Read"},{"name":"mike_roster"}],"max_tokens":16}"#;
    let patch = compute_envelope_patch(request_body, sent_body)?.ok_or_else(|| {
        anyhow::anyhow!("expected one envelope patch covering both `system` and `tools`")
    })?;
    assert_eq!(
        apply_one_patch(request_body, &patch),
        sent_body,
        "the single covering patch must reproduce the forwarded bytes exactly"
    );
    Ok(())
}

#[test]
fn compute_envelope_patch_covering_span_carries_an_interleaved_change() -> anyhow::Result<()> {
    let request_body = br#"{"messages":[],"system":"OLD","metadata":{"a":1},"tools":["OLD"]}"#;
    let sent_body = br#"{"messages":[],"system":"NEW","metadata":{"a":2},"tools":["NEW"]}"#;
    let patch = compute_envelope_patch(request_body, sent_body)?
        .ok_or_else(|| anyhow::anyhow!("expected a covering envelope patch"))?;
    assert_eq!(apply_one_patch(request_body, &patch), sent_body);
    Ok(())
}

#[test]
fn compute_envelope_patch_fails_closed_when_a_changed_key_was_added() -> anyhow::Result<()> {
    let request_body = br#"{"messages":[],"system":"OLD"}"#;
    let sent_body = br#"{"messages":[],"system":"NEW","tools":[{"name":"mike_roster"}]}"#;
    assert!(
        compute_envelope_patch(request_body, sent_body)?.is_none(),
        "an added top-level key has no clean span replace"
    );
    Ok(())
}

#[test]
fn multi_key_envelope_patch_composes_with_per_message_patches() -> anyhow::Result<()> {
    let request_body = br#"{"model":"m","messages":[{"role":"user","content":"hi"}],"system":[{"type":"text","text":"OLD-SYS"}],"tools":[{"name":"Read"}]}"#;
    let sent_body = br#"{"model":"m","messages":[{"role":"user","content":"hi EXTRA"}],"system":[{"type":"text","text":"NEW-SYS"}],"tools":[{"name":"Read"},{"name":"mike_roster"}]}"#;

    let envelope_patch = compute_envelope_patch(request_body, sent_body)?
        .ok_or_else(|| anyhow::anyhow!("expected a covering envelope patch"))?;
    let canonical = canonicalise_body_messages(request_body)?;
    let sent_patches = compute_sent_patches(request_body, sent_body, &canonical)?;
    let message_patch = sent_patches
        .get(&0)
        .ok_or_else(|| anyhow::anyhow!("expected an ord-0 sent patch"))?;

    let rows = vec![
        SentPatchRow {
            sent_patch_start: envelope_patch.start,
            sent_patch_end: envelope_patch.end,
            sent_patch_bytes: envelope_patch.bytes.clone(),
        },
        SentPatchRow {
            sent_patch_start: message_patch.start,
            sent_patch_end: message_patch.end,
            sent_patch_bytes: message_patch.bytes.clone(),
        },
    ];
    assert_eq!(
        apply_sent_patches(request_body, rows)?,
        sent_body,
        "the covering envelope patch and the per-message patch must not overlap, and \
             together must reproduce the forwarded bytes exactly"
    );
    Ok(())
}

#[test]
fn envelope_patch_and_sent_patch_together_reconstruct_a_combined_rewrite() -> anyhow::Result<()> {
    let request_body =
            br#"{"model":"m","system":[{"type":"text","text":"OLD-SYS"}],"messages":[{"role":"user","content":"hi"}]}"#;
    let sent_body =
            br#"{"model":"m","system":[{"type":"text","text":"NEW-SYS-REWRITTEN"}],"messages":[{"role":"user","content":"hi EXTRA"}]}"#;

    let envelope_patch = compute_envelope_patch(request_body, sent_body)?
        .ok_or_else(|| anyhow::anyhow!("expected an envelope patch covering `system`"))?;

    let canonical = canonicalise_body_messages(request_body)?;
    let sent_patches = compute_sent_patches(request_body, sent_body, &canonical)?;
    assert_eq!(sent_patches.len(), 1, "only ord 0's message differs");
    let message_patch = sent_patches
        .get(&0)
        .ok_or_else(|| anyhow::anyhow!("expected an ord-0 sent patch"))?;

    let rows = vec![
        SentPatchRow {
            sent_patch_start: envelope_patch.start,
            sent_patch_end: envelope_patch.end,
            sent_patch_bytes: envelope_patch.bytes.clone(),
        },
        SentPatchRow {
            sent_patch_start: message_patch.start,
            sent_patch_end: message_patch.end,
            sent_patch_bytes: message_patch.bytes.clone(),
        },
    ];
    let reconstructed = apply_sent_patches(request_body, rows)?;
    assert_eq!(
        reconstructed, sent_body,
        "the envelope patch and the per-message patch together must reproduce \
             the exact bytes forwarded upstream, byte-for-byte"
    );
    Ok(())
}
