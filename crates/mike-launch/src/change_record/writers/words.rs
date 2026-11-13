const COMMIT_MIN: usize = 7;
const COMMIT_MAX: usize = 40;
const MAX_COMMIT_CANDIDATES: usize = 8;
const EDGE_PUNCTUATION: &[char] = &[
    '`', '\'', '"', ',', ';', ':', '(', ')', '[', ']', '{', '}', '<', '>', '*', '!', '?',
];

fn trimmed(word: &str) -> &str {
    word.trim_end_matches('.')
        .trim_matches(EDGE_PUNCTUATION)
        .trim_end_matches('.')
}

pub fn commit_candidates(text: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for word in text.split_whitespace().map(trimmed) {
        let hex = (COMMIT_MIN..=COMMIT_MAX).contains(&word.len())
            && word
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase());
        let mixed = word.chars().any(|c| c.is_ascii_digit())
            && word.chars().any(|c| c.is_ascii_alphabetic());
        if hex && mixed && !found.iter().any(|f| f == word) {
            found.push(word.to_string());
        }
        if found.len() == MAX_COMMIT_CANDIDATES {
            break;
        }
    }
    found
}

fn without_line_suffix(word: &str) -> &str {
    let mut rest = word;
    while let Some((head, tail)) = rest.rsplit_once(':') {
        if tail.is_empty() || !tail.chars().all(|c| c.is_ascii_digit()) {
            break;
        }
        rest = head;
    }
    rest
}

pub fn path_candidates(text: &str, checkout: &str) -> Vec<String> {
    let prefix = format!("{}/", checkout.trim_end_matches('/'));
    let mut found: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        let word = trimmed(word);
        let word = without_line_suffix(word);
        let word = word.strip_prefix(&prefix).unwrap_or(word);
        let word = word.strip_prefix("./").unwrap_or(word);
        if word.contains('/') && !found.iter().any(|f| f == word) {
            found.push(word.to_string());
        }
    }
    found
}
