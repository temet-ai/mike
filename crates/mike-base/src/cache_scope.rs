use serde_json::Value;

pub fn is_globally_scoped(block: &Value) -> bool {
    block
        .get("cache_control")
        .and_then(|cc| cc.get("scope"))
        .and_then(Value::as_str)
        == Some("global")
}

pub fn last_global_index(blocks: &[Value]) -> Option<usize> {
    blocks.iter().rposition(is_globally_scoped)
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GlobalScopeViolation {
    #[error(
        "block at index {index} was part of the required-global prefix but is missing from the forwarded system array"
    )]
    BlockMissing { index: usize },
    #[error(
        "block at index {index} is at or before the last globally-scoped block but its bytes changed (mutated, or a block was inserted ahead of it)"
    )]
    BlockChanged { index: usize },
}

pub fn verify_global_prefix_preserved(
    original: &[Value],
    forwarded: &[Value],
) -> Result<(), GlobalScopeViolation> {
    let Some(last_idx) = last_global_index(original) else {
        return Ok(());
    };
    if forwarded.len() <= last_idx {
        return Err(GlobalScopeViolation::BlockMissing { index: last_idx });
    }
    for (idx, original_block) in original.iter().enumerate().take(last_idx + 1) {
        if *original_block != forwarded[idx] {
            return Err(GlobalScopeViolation::BlockChanged { index: idx });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn global_block(text: &str) -> Value {
        json!({"type": "text", "text": text, "cache_control": {"type": "ephemeral", "scope": "global", "ttl": "1h"}})
    }

    fn plain_block(text: &str) -> Value {
        json!({"type": "text", "text": text})
    }

    #[test]
    fn is_globally_scoped_true_only_for_scope_global() {
        assert!(is_globally_scoped(&global_block("x")));
        assert!(!is_globally_scoped(&plain_block("x")));
        assert!(!is_globally_scoped(
            &json!({"type": "text", "text": "x", "cache_control": {"type": "ephemeral"}})
        ));
    }

    #[test]
    fn last_global_index_finds_the_last_match() {
        let blocks = vec![
            plain_block("a"),
            global_block("b"),
            plain_block("c"),
            global_block("d"),
            plain_block("e"),
        ];
        assert_eq!(last_global_index(&blocks), Some(3));
    }

    #[test]
    fn last_global_index_none_when_no_global_block() {
        let blocks = vec![plain_block("a"), plain_block("b")];
        assert_eq!(last_global_index(&blocks), None);
    }

    #[test]
    fn verify_prefix_ok_when_no_global_block_present() {
        let original = vec![plain_block("a"), plain_block("b")];
        let forwarded = vec![plain_block("a"), plain_block("changed")];
        assert!(verify_global_prefix_preserved(&original, &forwarded).is_ok());
    }

    #[test]
    fn verify_prefix_ok_when_prefix_untouched() {
        let original = vec![plain_block("a"), global_block("b"), plain_block("c")];
        let forwarded = vec![
            plain_block("a"),
            global_block("b"),
            plain_block("c-rewritten"),
        ];
        assert!(verify_global_prefix_preserved(&original, &forwarded).is_ok());
    }

    #[test]
    fn verify_prefix_detects_mutated_global_block() {
        let original = vec![plain_block("a"), global_block("b")];
        let forwarded = vec![plain_block("a"), global_block("b-mutated")];
        assert_eq!(
            verify_global_prefix_preserved(&original, &forwarded),
            Err(GlobalScopeViolation::BlockChanged { index: 1 })
        );
    }

    #[test]
    fn verify_prefix_detects_insertion_before_global_block() {
        let original = vec![plain_block("a"), global_block("b")];
        let forwarded = vec![plain_block("a"), plain_block("injected"), global_block("b")];
        assert_eq!(
            verify_global_prefix_preserved(&original, &forwarded),
            Err(GlobalScopeViolation::BlockChanged { index: 1 })
        );
    }

    #[test]
    fn verify_prefix_detects_missing_global_block() {
        let original = vec![plain_block("a"), global_block("b")];
        let forwarded = vec![plain_block("a")];
        assert_eq!(
            verify_global_prefix_preserved(&original, &forwarded),
            Err(GlobalScopeViolation::BlockMissing { index: 1 })
        );
    }
}
