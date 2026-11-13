use std::sync::LazyLock;

use regex::Regex;

pub use crate::wire::text_span::TextSpan;

pub const ENDED_ON_VERSION: &str = "ended-on-v1";
pub const INPUT_CHARS: usize = 4000;

const WAIT_WINDOW: usize = 400;
const NEXT_STEP_WINDOW: usize = 250;
const HANDBACK_WINDOW: usize = 500;
const REPORT_WINDOW: usize = 600;

pub const DONE_VOCAB: [&str; 13] = [
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

fn compile(pattern: &str) -> Regex {
    match Regex::new(pattern) {
        Ok(re) => re,
        Err(e) => panic!("invalid ended_on pattern: {e}"),
    }
}

pub static WAIT: LazyLock<Regex> = LazyLock::new(|| {
    compile(
        r"(?i)\b(i'?ll|i will|we'?ll)\s+(report|check( back| in)?|come back|get back|follow up|pick (this|it) up|let you know|ping you|post|update you|resume|watch|monitor)\b|\b(waiting (for|on)|awaiting|once (it|they|that|the [\w-]+) (finish|finishes|completes?|lands?|reports?|returns?)|when (it|they|that) (finish|finishes|completes?|lands?|reports?)|next wake|still running|in the background|standing by|will be back)\b",
    )
});

pub static NEXT_STEP: LazyLock<Regex> = LazyLock::new(|| {
    compile(
        r"(?i)(^|\n|[.!]\s+)(now\s+)?(let me|let's|next,? i'?ll|i'?m going to|i'?ll now|now i'?ll|then i'?ll)\b",
    )
});

pub static HANDBACK: LazyLock<Regex> = LazyLock::new(|| {
    compile(
        r"(?i)\b(want me to|shall i|should i|would you like( me)? to|do you want( me)? to|let me know if you('d)? (like|want)|say the word|if you('d)? like,? i|your call|needs? your (decision|go-?ahead|approval|input)|awaiting your|which (option|one|approach) (do|would) you|ok to (proceed|push|commit|merge|deploy))\b",
    )
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndedOn {
    Report,
    Question,
    Handback,
    PromiseOrWait,
    Other,
    None,
}

impl EndedOn {
    pub fn code(self) -> &'static str {
        match self {
            EndedOn::Report => "report",
            EndedOn::Question => "question",
            EndedOn::Handback => "handback",
            EndedOn::PromiseOrWait => "promise_or_wait",
            EndedOn::Other => "other",
            EndedOn::None => "none",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndedOnClass {
    pub class: EndedOn,
    pub span: Option<(usize, usize)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Piece {
    text_start: usize,
    len: usize,
    source_start: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NonSidecarText {
    pub text: String,
    pieces: Vec<Piece>,
    dropped: usize,
}

pub fn is_py_space(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}')
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

pub fn char_len(text: &str) -> usize {
    text.chars().count()
}

fn char_offset(text: &str, byte: usize) -> usize {
    text[..byte].chars().count()
}

fn tail_start(text: &str, window: usize) -> usize {
    text.char_indices()
        .rev()
        .nth(window.saturating_sub(1))
        .map_or(0, |(index, _)| index)
}

pub fn find_bounded(haystack: &str, needle: &str) -> Option<usize> {
    let first = needle.chars().next()?;
    let last = needle.chars().next_back()?;
    let (starts_word, ends_word) = (is_word(first), is_word(last));
    let mut from = 0;
    while let Some(found) = haystack[from..].find(needle) {
        let start = from + found;
        let end = start + needle.len();
        let before_ok = !starts_word
            || haystack[..start]
                .chars()
                .next_back()
                .is_none_or(|c| !is_word(c));
        let after_ok = !ends_word || haystack[end..].chars().next().is_none_or(|c| !is_word(c));
        if before_ok && after_ok {
            return Some(start);
        }
        from = start + haystack[start..].chars().next().map_or(1, char::len_utf8);
    }
    None
}

fn is_line_break(c: char) -> bool {
    matches!(
        c,
        '\n' | '\r'
            | '\u{0b}'
            | '\u{0c}'
            | '\u{1c}'
            | '\u{1d}'
            | '\u{1e}'
            | '\u{85}'
            | '\u{2028}'
            | '\u{2029}'
    )
}

pub fn last_line(text: &str) -> &str {
    text.trim_matches(is_py_space)
        .split(is_line_break)
        .map(|line| line.trim_matches(is_py_space))
        .rfind(|line| !line.is_empty())
        .unwrap_or("")
}

fn find_in_window(re: &Regex, text: &str, window: usize) -> Option<(usize, usize)> {
    let start = tail_start(text, window);
    re.find(&text[start..])
        .map(|found| (start + found.start(), start + found.end()))
}

fn next_step_span(text: &str) -> Option<(usize, usize)> {
    let (start, end) = find_in_window(&NEXT_STEP, text, NEXT_STEP_WINDOW)?;
    let lead = text[start..end]
        .find(|c: char| !(c == '.' || c == '!' || is_py_space(c)))
        .unwrap_or(0);
    Some((start + lead, end))
}

fn wait_span(text: &str) -> Option<(usize, usize)> {
    find_in_window(&WAIT, text, WAIT_WINDOW)
        .or_else(|| next_step_span(text))
        .or_else(|| fragment_span(text))
}

fn fragment_span(text: &str) -> Option<(usize, usize)> {
    let suffix = ["...", ":", "\u{2026}"]
        .into_iter()
        .find(|suffix| text.ends_with(suffix))?;
    Some((text.len() - suffix.len(), text.len()))
}

fn question_span(text: &str) -> Option<(usize, usize)> {
    let line = last_line(text);
    if !line.ends_with('?') {
        return None;
    }
    let start = text.rfind(line)?;
    Some((start, start + line.len()))
}

fn report_span(text: &str) -> Option<(usize, usize)> {
    let start = tail_start(text, REPORT_WINDOW);
    let lowered = text[start..].to_lowercase();
    let (found, word) = DONE_VOCAB
        .into_iter()
        .filter_map(|word| find_bounded(&lowered, word).map(|at| (at, word)))
        .min_by_key(|(at, _)| *at)?;
    let from = char_offset(&lowered, found);
    let base = char_offset(text, start);
    Some((
        byte_of_char(text, base + from),
        byte_of_char(text, base + from + char_len(word)),
    ))
}

fn byte_of_char(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map_or(text.len(), |(index, _)| index)
}

fn to_chars((start, end): (usize, usize), text: &str) -> (usize, usize) {
    (char_offset(text, start), char_offset(text, end))
}

pub fn classify(text: &str) -> EndedOnClass {
    let trimmed = text.trim_end_matches(is_py_space);
    if trimmed.is_empty() {
        return EndedOnClass {
            class: EndedOn::None,
            span: None,
        };
    }
    let handback = find_in_window(&HANDBACK, trimmed, HANDBACK_WINDOW);
    let decided = handback
        .map(|span| (EndedOn::Handback, Some(span)))
        .or_else(|| question_span(trimmed).map(|span| (EndedOn::Question, Some(span))))
        .or_else(|| wait_span(trimmed).map(|span| (EndedOn::PromiseOrWait, Some(span))))
        .or_else(|| report_span(trimmed).map(|span| (EndedOn::Report, Some(span))))
        .unwrap_or((EndedOn::Other, None));
    EndedOnClass {
        class: decided.0,
        span: decided.1.map(|span| to_chars(span, trimmed)),
    }
}

fn slice_chars(text: &str, start: usize, end: usize) -> &str {
    let from = byte_of_char(text, start);
    let to = byte_of_char(text, end.max(start));
    &text[from..to]
}

pub fn non_sidecar_text(final_text: &str, spans: &[TextSpan]) -> NonSidecarText {
    let total = char_len(final_text);
    let mut full = String::new();
    let mut pieces = Vec::new();
    let mut filled = 0;
    for span in spans.iter().filter(|span| !span.is_sidecar()) {
        let (start, end) = (span.start.min(total), span.end.min(total));
        let piece = slice_chars(final_text, start, end);
        let len = char_len(piece);
        pieces.push(Piece {
            text_start: filled,
            len,
            source_start: start,
        });
        full.push_str(piece);
        filled += len;
    }
    let dropped = filled.saturating_sub(INPUT_CHARS);
    let text = slice_chars(&full, dropped, filled).to_string();
    NonSidecarText {
        text,
        pieces,
        dropped,
    }
}

impl NonSidecarText {
    fn to_source(&self, offset: usize, is_end: bool) -> usize {
        let full = offset + self.dropped;
        let probe = if is_end { full.saturating_sub(1) } else { full };
        let piece = self
            .pieces
            .iter()
            .rfind(|piece| piece.text_start <= probe && probe < piece.text_start + piece.len);
        match piece {
            Some(piece) => piece.source_start + (probe - piece.text_start) + usize::from(is_end),
            None => full,
        }
    }

    pub fn classify(&self) -> EndedOnClass {
        let class = classify(&self.text);
        EndedOnClass {
            class: class.class,
            span: class
                .span
                .map(|(start, end)| (self.to_source(start, false), self.to_source(end, true))),
        }
    }
}
