pub fn head_chars(text: &str, max: usize) -> &str {
    match text.char_indices().nth(max) {
        Some((at, _)) => &text[..at],
        None => text,
    }
}

pub fn tail_chars(text: &str, max: usize) -> &str {
    let total = text.chars().count();
    match text.char_indices().nth(total.saturating_sub(max)) {
        Some((at, _)) => &text[at..],
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_chars_cuts_the_end_on_a_character_boundary() {
        assert_eq!(head_chars("héllo wörld", 5), "héllo");
        assert_eq!(head_chars("short", 50), "short");
    }

    #[test]
    fn tail_chars_keeps_the_end_on_a_character_boundary() {
        assert_eq!(tail_chars("héllo wörld", 5), "wörld");
        assert_eq!(tail_chars("short", 50), "short");
        assert_eq!(tail_chars("", 3), "");
    }
}
