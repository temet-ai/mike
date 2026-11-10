use std::collections::BTreeSet;

const MIN_WORD_CHARS: usize = 3;
const MIN_TICKET_PREFIX_CHARS: usize = 2;
const MIN_TICKET_DIGITS: usize = 2;

const DEICTIC_PHRASES: &[&str] = &[
    "continue",
    "continuing",
    "carry on",
    "carrying on",
    "where are we",
    "where were we",
    "where we are",
    "where we left",
    "left off",
    "what's the status",
    "whats the status",
    "what is the status",
    "status",
    "pick up",
    "pick this up",
    "pick it up",
    "picking up",
    "resume",
    "resuming",
    "the open epic",
    "the open thread",
    "the open work",
    "the open task",
    "the open item",
    "that epic",
    "that thread",
    "that decision",
    "the last decision",
];

const STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "that", "this", "from", "have", "has", "are", "was", "were",
    "can", "you", "our", "your", "what", "where", "how", "why", "when", "will", "would", "should",
    "could", "let", "lets", "please", "about", "into", "its", "not", "but", "all", "any", "now",
    "then", "there", "here", "who", "which", "just", "also", "get", "got", "new", "one", "two",
    "out", "too", "their", "them", "they", "been", "did", "does", "doing", "done", "want", "need",
    "continue", "resume", "open", "work", "thread", "epic", "status", "carry", "pick", "back",
    "last", "thing", "still", "left", "off", "decision",
];

pub(super) struct Text {
    lower: String,
    tickets: Vec<String>,
}

impl Text {
    pub(super) fn new(raw: &str) -> Self {
        Self {
            lower: raw.to_lowercase(),
            tickets: tickets(raw),
        }
    }

    pub(super) fn mentions(&self, slug: &str, haystack: &str) -> bool {
        contains_bounded(&self.lower, slug)
            || self
                .tickets
                .iter()
                .any(|ticket| contains_bounded(haystack, ticket))
    }
}

impl Text {
    pub(super) fn mentions_phrase(&self, phrase: &str) -> bool {
        contains_bounded(&self.lower, phrase)
    }
}

pub(super) fn is_deictic(prompt: &str) -> bool {
    let normalised = prompt.to_lowercase().replace('\u{2019}', "'");
    DEICTIC_PHRASES
        .iter()
        .any(|phrase| contains_bounded(&normalised, phrase))
}

pub(super) fn tokenize(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|word| {
            word.chars().count() >= MIN_WORD_CHARS && !STOPWORDS.contains(&word.as_str())
        })
        .collect()
}

pub(super) fn tickets(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .filter(|token| is_ticket(token))
        .map(str::to_lowercase)
        .collect()
}

fn is_ticket(token: &str) -> bool {
    let Some((prefix, number)) = token.rsplit_once('-') else {
        return false;
    };
    prefix.len() >= MIN_TICKET_PREFIX_CHARS
        && prefix.chars().all(|c| c.is_ascii_uppercase())
        && number.len() >= MIN_TICKET_DIGITS
        && number.chars().all(|c| c.is_ascii_digit())
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '-' || c == '_'
}

pub(super) fn contains_bounded(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    haystack.match_indices(needle).any(|(start, matched)| {
        let before = haystack[..start].chars().next_back();
        let after = haystack[start + matched.len()..].chars().next();
        !before.is_some_and(is_word_char) && !after.is_some_and(is_word_char)
    })
}

#[cfg(test)]
#[path = "resolve_text_tests.rs"]
mod tests;
