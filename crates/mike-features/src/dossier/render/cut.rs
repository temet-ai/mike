pub fn char_count(text: &str) -> usize {
    text.chars().count()
}

pub fn head_chars(text: &str, n: usize) -> &str {
    match text.char_indices().nth(n) {
        Some((byte, _)) => &text[..byte],
        None => text,
    }
}

pub fn tail_chars(text: &str, n: usize) -> &str {
    let total = char_count(text);
    if n >= total {
        return text;
    }
    match text.char_indices().nth(total - n) {
        Some((byte, _)) => &text[byte..],
        None => "",
    }
}

pub fn slice_chars(text: &str, start: usize, end: usize) -> &str {
    let from = text
        .char_indices()
        .nth(start)
        .map_or(text.len(), |(b, _)| b);
    let to = text
        .char_indices()
        .nth(end.max(start))
        .map_or(text.len(), |(b, _)| b);
    &text[from..to]
}

pub fn cut(text: &str, limit: usize) -> String {
    let total = char_count(text);
    if total <= limit {
        return text.to_owned();
    }
    format!(
        "{} [... {} more characters]",
        head_chars(text, limit),
        total - limit
    )
}

pub fn cut_front(text: &str, limit: usize) -> String {
    let total = char_count(text);
    if total <= limit {
        return text.to_owned();
    }
    format!(
        "[... {} earlier characters not shown] {}",
        total - limit,
        tail_chars(text, limit)
    )
}

pub fn keep_last(items: Vec<String>, keep: usize, noun: &str) -> Vec<String> {
    let omitted = items.len().saturating_sub(keep);
    let mut out = Vec::with_capacity(keep + 1);
    if omitted > 0 {
        out.push(format!("({omitted} earlier {noun} not shown)"));
    }
    out.extend(items.into_iter().skip(omitted));
    out
}

pub fn body(lines: &[String]) -> String {
    if lines.is_empty() {
        "(none)".to_owned()
    } else {
        lines.join("\n\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cut_counts_unicode_scalars_and_announces_the_rest() {
        assert_eq!(cut("héllo", 10), "héllo");
        assert_eq!(cut("héllo wörld", 5), "héllo [... 6 more characters]");
    }

    #[test]
    fn cut_front_keeps_the_tail() {
        assert_eq!(
            cut_front("abcdef", 2),
            "[... 4 earlier characters not shown] ef"
        );
    }

    #[test]
    fn keep_last_prefixes_the_omitted_count() {
        let items: Vec<String> = (1..=5).map(|i| i.to_string()).collect();
        assert_eq!(
            keep_last(items, 2, "things are"),
            vec!["(3 earlier things are not shown)", "4", "5"]
        );
        assert_eq!(keep_last(vec!["a".to_owned()], 2, "x"), vec!["a"]);
    }

    #[test]
    fn body_joins_or_says_none() {
        assert_eq!(body(&[]), "(none)");
        assert_eq!(body(&["a".to_owned(), "b".to_owned()]), "a\n\nb");
    }
}
