const DONE_CLAIM_VOCABULARY: &[&str] = &[
    "done",
    "complete",
    "completed",
    "finished",
    "shipped",
    "landed",
    "merged",
    "deployed",
    "implemented",
    "resolved",
    "verified",
    "fixed",
    "ready",
];

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

pub(super) fn contains_bounded(haystack_lower: &str, needle_lower: &str) -> bool {
    let starts_with_word = needle_lower.chars().next().is_some_and(is_word_char);
    let ends_with_word = needle_lower.chars().next_back().is_some_and(is_word_char);
    if needle_lower.is_empty() {
        return false;
    }
    haystack_lower
        .match_indices(needle_lower)
        .any(|(start, matched)| {
            let boundary_before = !starts_with_word
                || haystack_lower[..start]
                    .chars()
                    .next_back()
                    .is_none_or(|c| !is_word_char(c));
            let boundary_after = !ends_with_word
                || haystack_lower[start + matched.len()..]
                    .chars()
                    .next()
                    .is_none_or(|c| !is_word_char(c));
            boundary_before && boundary_after
        })
}

pub(super) fn has_done_claim_wording(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    let lower = text.to_lowercase();
    DONE_CLAIM_VOCABULARY
        .iter()
        .any(|word| contains_bounded(&lower, word))
}
