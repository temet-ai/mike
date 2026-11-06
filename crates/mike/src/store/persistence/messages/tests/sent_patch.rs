use serde_json::json;

use crate::store::persistence::PersistError;
use crate::store::persistence::messages::sent_patch::locate_message_spans;
use crate::store::persistence::messages::{
    CanonicalMessage, SentPatchRow, apply_sent_patches, canonicalise_body_messages,
    compute_sent_patches,
};

#[test]
fn locate_message_spans_recovers_exact_byte_ranges() -> anyhow::Result<()> {
    let body = br#"{"model":"x","messages":[{"role":"user","content":"a"},{"role":"assistant","content":"b"}]}"#;
    let spans = locate_message_spans(body)?;
    assert_eq!(spans.len(), 2);
    assert_eq!(
        &body[spans[0].start..spans[0].end],
        br#"{"role":"user","content":"a"}"#
    );
    assert_eq!(
        &body[spans[1].start..spans[1].end],
        br#"{"role":"assistant","content":"b"}"#
    );
    Ok(())
}

#[test]
fn locate_message_spans_empty_for_missing_messages_field() -> anyhow::Result<()> {
    let spans = locate_message_spans(br#"{"model":"x"}"#)?;
    assert!(spans.is_empty());
    Ok(())
}

#[test]
fn compute_sent_patches_detects_only_the_mutated_ord() -> anyhow::Result<()> {
    let request_body = json!({
        "messages": [
            {"role": "user", "content": "first"},
            {"role": "assistant", "content": "second"}
        ]
    })
    .to_string()
    .into_bytes();
    let sent_body = json!({
        "messages": [
            {"role": "user", "content": "first MUTATED"},
            {"role": "assistant", "content": "second"}
        ]
    })
    .to_string()
    .into_bytes();
    let canonical = canonicalise_body_messages(&request_body)?;
    let patches = compute_sent_patches(&request_body, &sent_body, &canonical)?;
    assert_eq!(patches.len(), 1, "only ord 0 differs");
    let patch = &patches[&0];
    assert_eq!(
        &request_body[patch.start as usize..patch.end as usize],
        br#"{"content":"first","role":"user"}"#
    );
    assert_eq!(
        patch.bytes,
        br#"{"content":"first MUTATED","role":"user"}"#.to_vec()
    );
    Ok(())
}

#[test]
fn compute_sent_patches_empty_when_byte_identical() -> anyhow::Result<()> {
    let body = json!({"messages": [{"role": "user", "content": "same"}]})
        .to_string()
        .into_bytes();
    let canonical = canonicalise_body_messages(&body)?;
    let patches = compute_sent_patches(&body, &body, &canonical)?;
    assert!(patches.is_empty());
    Ok(())
}

#[test]
fn compute_sent_patches_fails_closed_on_message_count_mismatch() -> anyhow::Result<()> {
    let request_body = json!({"messages": [{"role": "user", "content": "one"}]})
        .to_string()
        .into_bytes();
    let sent_body = json!({
        "messages": [
            {"role": "user", "content": "one"},
            {"role": "user", "content": "INSERTED"}
        ]
    })
    .to_string()
    .into_bytes();
    let canonical = canonicalise_body_messages(&request_body)?;
    let patches = compute_sent_patches(&request_body, &sent_body, &canonical)?;
    assert!(
        patches.is_empty(),
        "a message-count mismatch cannot be aligned by ord; must not guess"
    );
    Ok(())
}

#[test]
fn apply_sent_patches_splices_replacement_and_preserves_surrounding_bytes() -> anyhow::Result<()> {
    let original = br#"{"a":1,"messages":[{"role":"user","content":"first"},{"role":"assistant","content":"second"}]}"#.to_vec();
    let request_body = original.clone();
    let spans = locate_message_spans(&request_body)?;
    let patch = SentPatchRow {
        sent_patch_start: spans[0].start as i32,
        sent_patch_end: spans[0].end as i32,
        sent_patch_bytes: br#"{"role":"user","content":"first MUTATED"}"#.to_vec(),
    };
    let result = apply_sent_patches(&original, vec![patch])?;
    let expected = br#"{"a":1,"messages":[{"role":"user","content":"first MUTATED"},{"role":"assistant","content":"second"}]}"#.to_vec();
    assert_eq!(result, expected);
    Ok(())
}

#[test]
fn apply_sent_patches_noop_returns_original() -> anyhow::Result<()> {
    let original = b"unchanged bytes".to_vec();
    let result = apply_sent_patches(&original, Vec::new())?;
    assert_eq!(result, original);
    Ok(())
}

#[test]
#[allow(
    clippy::unwrap_used,
    reason = "test asserts the Err variant of a call that must fail"
)]
fn apply_sent_patches_rejects_overlapping_patches() {
    let original = b"0123456789".to_vec();
    let patches = vec![
        SentPatchRow {
            sent_patch_start: 0,
            sent_patch_end: 5,
            sent_patch_bytes: b"AAAAA".to_vec(),
        },
        SentPatchRow {
            sent_patch_start: 3,
            sent_patch_end: 8,
            sent_patch_bytes: b"BBBBB".to_vec(),
        },
    ];
    let err = apply_sent_patches(&original, patches).unwrap_err();
    assert!(matches!(err, PersistError::DataInvariant(_)));
}

#[test]
#[allow(
    clippy::unwrap_used,
    reason = "test asserts the Err variant of a call that must fail"
)]
fn apply_sent_patches_rejects_out_of_range_patch() {
    let original = b"short".to_vec();
    let patches = vec![SentPatchRow {
        sent_patch_start: 0,
        sent_patch_end: 999,
        sent_patch_bytes: b"x".to_vec(),
    }];
    let err = apply_sent_patches(&original, patches).unwrap_err();
    assert!(matches!(err, PersistError::DataInvariant(_)));
}

#[test]
#[allow(
    clippy::expect_used,
    reason = "test fixture; the patch it indexes was just asserted to exist"
)]
fn locate_message_spans_and_apply_sent_patches_round_trip_a_message_mutation() -> anyhow::Result<()>
{
    let request_body =
        br#"{"messages":[{"role":"user","content":"hi"},{"role":"assistant","content":"SHORT"}]}"#;
    let sent_body =
            br#"{"messages":[{"role":"user","content":"hi"},{"role":"assistant","content":"MUCH-LONGER-REPLY"}]}"#;
    let canonical = vec![
        CanonicalMessage {
            sha: "s0".to_string(),
            body_canonical_bytes: vec![],
            byte_size: 0,
            role: "user".to_string(),
            ord: 0,
            cache_control: None,
        },
        CanonicalMessage {
            sha: "s1".to_string(),
            body_canonical_bytes: vec![],
            byte_size: 0,
            role: "assistant".to_string(),
            ord: 1,
            cache_control: None,
        },
    ];
    let mut patches = compute_sent_patches(request_body, sent_body, &canonical)?;
    assert_eq!(patches.len(), 1, "only ord 1 changed: {patches:?}");
    let patch = patches.remove(&1).expect("patch for ord 1");
    let rows = vec![SentPatchRow {
        sent_patch_start: patch.start,
        sent_patch_end: patch.end,
        sent_patch_bytes: patch.bytes,
    }];
    let reconstructed = apply_sent_patches(request_body, rows)?;
    assert_eq!(reconstructed, sent_body);
    Ok(())
}
