fn is_py_space(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}')
}

pub fn split_sentences(text: &str) -> Vec<(usize, String)> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < n {
        let after_stop = i > 0 && matches!(chars[i - 1], '.' | '!' | '?');
        let splits_on_space = after_stop && is_py_space(chars[i]);
        if splits_on_space || chars[i] == '\n' {
            push_piece(&chars, start, i, &mut out);
            let mut j = i;
            while j < n
                && (if splits_on_space {
                    is_py_space(chars[j])
                } else {
                    chars[j] == '\n'
                })
            {
                j += 1;
            }
            start = j;
            i = j;
            continue;
        }
        i += 1;
    }
    push_piece(&chars, start, n, &mut out);
    out
}

fn push_piece(chars: &[char], start: usize, end: usize, out: &mut Vec<(usize, String)>) {
    let piece: String = chars[start..end].iter().collect();
    let lead = piece.chars().take_while(|c| is_py_space(*c)).count();
    out.push((start + lead, piece.trim_matches(is_py_space).to_owned()));
}

pub fn sentence_containing(text: &str, char_offset: usize) -> Option<String> {
    let pieces = split_sentences(text);
    pieces
        .iter()
        .rev()
        .find(|(start, _)| *start <= char_offset)
        .map(|(_, sentence)| sentence.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(text: &str) -> Vec<String> {
        split_sentences(text).into_iter().map(|(_, s)| s).collect()
    }

    #[test]
    fn splits_after_terminal_punctuation_followed_by_space() {
        assert_eq!(
            texts("Fix it. Then commit! Why? ok"),
            vec!["Fix it.", "Then commit!", "Why?", "ok"]
        );
    }

    #[test]
    fn splits_at_runs_of_newlines() {
        assert_eq!(texts("one\n\n\ntwo\nthree"), vec!["one", "two", "three"]);
    }

    #[test]
    fn does_not_split_inside_a_version_number() {
        assert_eq!(texts("bump to 1.2 please"), vec!["bump to 1.2 please"]);
    }

    #[test]
    fn finds_the_sentence_that_holds_an_offset() {
        let text = "Refactor the store. Then push when green.";
        assert_eq!(
            sentence_containing(text, 25).as_deref(),
            Some("Then push when green.")
        );
        assert_eq!(
            sentence_containing(text, 0).as_deref(),
            Some("Refactor the store.")
        );
    }
}
