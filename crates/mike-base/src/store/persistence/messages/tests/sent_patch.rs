use crate::store::persistence::PersistError;
use crate::store::persistence::messages::{SentPatchRow, apply_sent_patches};

#[test]
fn apply_sent_patches_splices_replacement_and_preserves_surrounding_bytes() -> anyhow::Result<()> {
    let original = br#"{"a":1,"messages":[{"role":"user","content":"first"},{"role":"assistant","content":"second"}]}"#.to_vec();
    let patch = SentPatchRow {
        sent_patch_start: 19,
        sent_patch_end: 52,
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
