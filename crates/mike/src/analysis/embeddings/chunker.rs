pub(crate) fn chunk_text(text: &str, max_chars: usize) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let cap = max_chars.max(1);
    let total_chars = text.chars().count();
    if total_chars <= cap {
        return vec![text.to_string()];
    }

    let mut out: Vec<String> = Vec::new();
    let mut remaining: &str = text;
    loop {
        let remaining_chars = remaining.chars().count();
        if remaining_chars == 0 {
            break;
        }
        if remaining_chars <= cap {
            out.push(remaining.to_string());
            break;
        }
        let boundary_byte = remaining
            .char_indices()
            .nth(cap)
            .map(|(idx, _)| idx)
            .unwrap_or(remaining.len());
        let head = &remaining[..boundary_byte];
        let split_byte = find_split_index(head).unwrap_or(boundary_byte);
        let chunk = remaining[..split_byte].trim_end();
        if !chunk.is_empty() {
            out.push(chunk.to_string());
        }
        let after = &remaining[split_byte..];
        remaining = after.trim_start();
    }
    out
}

fn find_split_index(head: &str) -> Option<usize> {
    if let Some(idx) = head.rfind("\n\n") {
        return Some(idx);
    }
    let mut sentence_end: Option<usize> = None;
    for (idx, ch) in head.char_indices() {
        if matches!(ch, '.' | '!' | '?') {
            let after = idx + ch.len_utf8();
            if let Some(next_ch) = head[after..].chars().next()
                && next_ch.is_whitespace()
            {
                sentence_end = Some(after);
            }
        }
    }
    if let Some(idx) = sentence_end {
        return Some(idx);
    }
    head.char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace())
        .map(|(idx, _)| idx)
}
