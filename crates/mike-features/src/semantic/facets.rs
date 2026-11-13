use crate::dossier::build::assemble::parse_spans;

fn chars(text: &str) -> usize {
    text.chars().count()
}

fn tail_chars(text: &str, n: usize) -> &str {
    let total = chars(text);
    if total <= n {
        return text;
    }
    let skip = total - n;
    let start = text.char_indices().nth(skip).map_or(text.len(), |(i, _)| i);
    &text[start..]
}

fn cut_back_to_whitespace(tail: &str, was_cut: bool) -> &str {
    if !was_cut {
        return tail.trim();
    }
    match tail.find(char::is_whitespace) {
        Some(i) => tail[i..].trim(),
        None => tail.trim(),
    }
}

fn span_text(final_text: &str, start: usize, end: usize) -> String {
    final_text
        .chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}

pub fn non_sidecar(final_text: &str, text_spans: &serde_json::Value) -> String {
    let spans = parse_spans(text_spans);
    if spans.is_empty() {
        return final_text.to_owned();
    }
    spans
        .iter()
        .filter(|s| !s.is_sidecar())
        .map(|s| span_text(final_text, s.start, s.end))
        .collect()
}

pub fn ending_text(final_text: &str, text_spans: &serde_json::Value, max_chars: usize) -> String {
    let body = non_sidecar(final_text, text_spans);
    let tail = tail_chars(&body, max_chars);
    cut_back_to_whitespace(tail, chars(&body) > max_chars).to_owned()
}

pub fn prompt_head(user_prompt: &str, chunk_max_chars: usize) -> Option<String> {
    crate::analysis::embeddings::chunk_text(user_prompt, chunk_max_chars)
        .into_iter()
        .next()
        .filter(|c| !c.trim().is_empty())
}
