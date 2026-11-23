use crate::store::persistence::PersistError;

use super::canonicalize::invariant;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SentPatchRow {
    pub sent_patch_start: i32,
    pub sent_patch_end: i32,
    pub sent_patch_bytes: Vec<u8>,
}

pub fn apply_sent_patches(
    original: &[u8],
    mut patches: Vec<SentPatchRow>,
) -> Result<Vec<u8>, PersistError> {
    patches.sort_by_key(|p| p.sent_patch_start);
    let mut out = Vec::with_capacity(original.len());
    let mut cursor: usize = 0;
    for p in &patches {
        let start = usize::try_from(p.sent_patch_start)
            .map_err(|_| invariant("sent_patch_start is negative"))?;
        let end = usize::try_from(p.sent_patch_end)
            .map_err(|_| invariant("sent_patch_end is negative"))?;
        if start < cursor || end > original.len() || start > end {
            return Err(invariant(format!(
                "sent patch [{start}, {end}) is out of range or overlaps a prior patch \
                 (cursor={cursor}, original.len()={})",
                original.len()
            )));
        }
        out.extend_from_slice(&original[cursor..start]);
        out.extend_from_slice(&p.sent_patch_bytes);
        cursor = end;
    }
    out.extend_from_slice(&original[cursor..]);
    Ok(out)
}
