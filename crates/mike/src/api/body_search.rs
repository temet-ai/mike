use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct TextMatch {
    pub(crate) byte_offset: u64,
    pub(crate) match_length: u32,
    pub(crate) line: u32,
    pub(crate) column: u32,
    pub(crate) before: String,
    pub(crate) matched: String,
    pub(crate) after: String,
}

pub(crate) fn find_in_str(
    haystack: &str,
    needle: &str,
    case_sensitive: bool,
    context_bytes: u32,
    max_matches: u32,
) -> (u64, Vec<TextMatch>) {
    let offsets = match_offsets(haystack, needle, case_sensitive);
    let matches = offsets
        .iter()
        .take(max_matches as usize)
        .map(|&offset| text_match_at(haystack, offset, needle.len(), context_bytes))
        .collect();
    (offsets.len() as u64, matches)
}

pub(crate) fn match_offsets(haystack: &str, needle: &str, case_sensitive: bool) -> Vec<usize> {
    if needle.is_empty() {
        return Vec::new();
    }
    let (search_haystack, search_needle): (String, String) = if case_sensitive {
        (haystack.to_string(), needle.to_string())
    } else {
        (haystack.to_ascii_lowercase(), needle.to_ascii_lowercase())
    };
    let mut offsets = Vec::new();
    let mut start = 0usize;
    while start <= search_haystack.len() {
        let Some(rel) = search_haystack[start..].find(&search_needle) else {
            break;
        };
        let absolute = start + rel;
        offsets.push(absolute);
        start = absolute + needle.len().max(1);
    }
    offsets
}

pub(crate) fn text_match_at(
    haystack: &str,
    absolute: usize,
    needle_len: usize,
    context_bytes: u32,
) -> TextMatch {
    let context_window = context_bytes as usize;
    let (line, column) = line_column_for_offset(haystack, absolute);
    TextMatch {
        byte_offset: absolute as u64,
        match_length: u32::try_from(needle_len).unwrap_or(u32::MAX),
        line,
        column,
        before: slice_before(haystack, absolute, context_window).to_string(),
        matched: haystack[absolute..absolute + needle_len].to_string(),
        after: slice_after(haystack, absolute + needle_len, context_window).to_string(),
    }
}

fn slice_before(haystack: &str, match_start: usize, window: usize) -> &str {
    if match_start == 0 || window == 0 {
        return "";
    }
    let raw_start = match_start.saturating_sub(window);
    let mut start = raw_start;
    while start < match_start && !haystack.is_char_boundary(start) {
        start += 1;
    }
    &haystack[start..match_start]
}

fn slice_after(haystack: &str, match_end: usize, window: usize) -> &str {
    if match_end >= haystack.len() || window == 0 {
        return "";
    }
    let raw_end = (match_end + window).min(haystack.len());
    let mut end = raw_end;
    while end > match_end && !haystack.is_char_boundary(end) {
        end -= 1;
    }
    &haystack[match_end..end]
}

fn line_column_for_offset(haystack: &str, target: usize) -> (u32, u32) {
    let mut line: u32 = 1;
    let mut line_start: usize = 0;
    let bytes = haystack.as_bytes();
    let upper = target.min(bytes.len());
    for (i, &b) in bytes[..upper].iter().enumerate() {
        if b == b'\n' {
            line = line.saturating_add(1);
            line_start = i + 1;
        }
    }
    let column_chars = haystack
        .get(line_start..target)
        .map(|s| s.chars().count())
        .unwrap_or(0)
        .saturating_add(1);
    let column = u32::try_from(column_chars).unwrap_or(u32::MAX);
    (line, column)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_needle_returns_zero_count_and_no_matches() {
        let (count, matches) = find_in_str("hello world", "", true, 0, 10);
        assert_eq!(count, 0);
        assert!(matches.is_empty());
    }

    #[test]
    fn single_match_carries_round_trip_offset() {
        let (count, matches) = find_in_str("hello world", "world", true, 0, 10);
        assert_eq!(count, 1);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].byte_offset, 6);
        assert_eq!(matches[0].match_length, 5);
        assert_eq!(matches[0].matched, "world");
        let h = "hello world";
        let slice = &h[matches[0].byte_offset as usize
            ..matches[0].byte_offset as usize + matches[0].match_length as usize];
        assert_eq!(slice, matches[0].matched);
    }

    #[test]
    fn context_window_is_utf8_safe() {
        let h = "préfixe match suffixé";
        let (count, matches) = find_in_str(h, "match", true, 4, 10);
        assert_eq!(count, 1);
        let m = &matches[0];
        assert!(!m.before.is_empty());
        assert!(!m.after.is_empty());
    }

    #[test]
    fn multiple_matches_capped_by_max_matches_keeps_full_count_honest() {
        let h = "abcabcabcabc";
        let (count, matches) = find_in_str(h, "abc", true, 0, 2);
        assert_eq!(count, 4, "full_match_count must reflect every hit");
        assert_eq!(matches.len(), 2, "page must be capped at max_matches");
    }

    #[test]
    fn case_insensitive_matches_ascii_fold() {
        let h = "Hello WORLD";
        let (count, matches) = find_in_str(h, "world", false, 0, 10);
        assert_eq!(count, 1);
        assert_eq!(matches[0].matched, "WORLD");
    }

    #[test]
    fn line_column_reports_one_based_for_first_line() {
        let h = "abc match xyz";
        let (_, matches) = find_in_str(h, "match", true, 0, 10);
        assert_eq!(matches[0].line, 1);
        assert_eq!(matches[0].column, 5);
    }

    #[test]
    fn line_column_reports_one_based_for_later_lines() {
        let h = "first\nsecond\nthird match\nlast";
        let (_, matches) = find_in_str(h, "match", true, 0, 10);
        assert_eq!(matches[0].line, 3);
        assert_eq!(matches[0].column, 7);
    }

    #[test]
    fn match_offsets_lists_every_non_overlapping_start() {
        assert_eq!(match_offsets("abcabcabc", "abc", true), vec![0, 3, 6]);
        assert_eq!(match_offsets("aaaa", "aa", true), vec![0, 2]);
        assert!(match_offsets("abc", "", true).is_empty());
    }

    #[test]
    fn match_offsets_case_insensitive_folds_ascii_only() {
        assert_eq!(match_offsets("Foo fOO", "foo", false), vec![0, 4]);
        assert!(match_offsets("Foo fOO", "foo", true).is_empty());
    }

    #[test]
    fn text_match_at_carries_line_column_and_context() {
        let h = "one\ntwo match three";
        let m = text_match_at(h, 8, 5, 3);
        assert_eq!(m.byte_offset, 8);
        assert_eq!(m.line, 2);
        assert_eq!(m.column, 5);
        assert_eq!(m.matched, "match");
        assert_eq!(m.before, "wo ");
        assert_eq!(m.after, " th");
    }

    #[test]
    fn no_matches_yields_zero_count_and_empty_vec() {
        let (count, matches) = find_in_str("abc", "z", true, 0, 10);
        assert_eq!(count, 0);
        assert!(matches.is_empty());
    }
}
