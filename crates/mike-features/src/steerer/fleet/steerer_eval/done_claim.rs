use crate::dossier::build::ended_on::{DONE_VOCAB, find_bounded};

pub fn contains_bounded(haystack_lower: &str, needle_lower: &str) -> bool {
    find_bounded(haystack_lower, needle_lower).is_some()
}

pub fn has_done_claim_wording(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    let lower = text.to_lowercase();
    DONE_VOCAB.iter().any(|word| contains_bounded(&lower, word))
}
