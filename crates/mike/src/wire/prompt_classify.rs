use sha2::{Digest, Sha256};

pub(crate) fn prompt_sha256_from_block_text(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let digest = hasher.finalize();
    hex::encode(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_sha256_is_sha256_hex_of_block_text() {
        let id = prompt_sha256_from_block_text("hello world");
        assert_eq!(
            id,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn prompt_sha256_empty_string_is_well_known() {
        let id = prompt_sha256_from_block_text("");
        assert_eq!(
            id,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn prompt_sha256_is_lowercase_hex() {
        let id = prompt_sha256_from_block_text("any-input");
        assert!(
            id.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
            "prompt_sha256 must be lowercase hex, got: {id}"
        );
        assert_eq!(id.len(), 64, "sha256 hex must be 64 chars");
    }

    #[test]
    fn prompt_sha256_is_deterministic() {
        let a = prompt_sha256_from_block_text("the quick brown fox");
        let b = prompt_sha256_from_block_text("the quick brown fox");
        assert_eq!(a, b);
    }

    #[test]
    fn prompt_sha256_is_input_sensitive() {
        let a = prompt_sha256_from_block_text("the quick brown fox");
        let b = prompt_sha256_from_block_text("the quick brown fox.");
        assert_ne!(a, b, "single-char delta must change the sha");
    }
}
