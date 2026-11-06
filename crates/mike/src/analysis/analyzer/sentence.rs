use regex::Regex;

pub(super) fn sentence_matches_any_filter(
    text: &str,
    match_range: (usize, usize),
    filters: &[Regex],
) -> bool {
    if filters.is_empty() {
        return false;
    }
    let (start, end) = match_range;
    let start_bound = nearest_sentence_start(text, start);
    let end_bound = nearest_sentence_end(text, end);
    let sentence = match text.get(start_bound..end_bound) {
        Some(s) => s,
        None => text,
    };
    filters.iter().any(|f| f.is_match(sentence))
}

fn nearest_sentence_start(text: &str, pos: usize) -> usize {
    let mut cut = 0usize;
    for (i, ch) in text.char_indices() {
        if i >= pos {
            break;
        }
        if matches!(ch, '.' | '!' | '?' | '\n') {
            cut = i + ch.len_utf8();
        }
    }
    let rest = &text[cut..];
    let trimmed = rest.trim_start();
    cut + (rest.len() - trimmed.len())
}

fn nearest_sentence_end(text: &str, pos: usize) -> usize {
    for (i, ch) in text.char_indices() {
        if i < pos {
            continue;
        }
        if matches!(ch, '.' | '!' | '?' | '\n') {
            return i + ch.len_utf8();
        }
    }
    text.len()
}
