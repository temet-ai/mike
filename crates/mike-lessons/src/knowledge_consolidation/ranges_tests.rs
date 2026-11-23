use super::ranges::{expand_to_lines, substantive_chars, worth_splitting};

const LONG: &str = "This is a long enough paragraph of real instructions that goes comfortably past the eighty character floor, with extra words.\n";

#[test]
fn ranges_expand_to_whole_lines() {
    let body = "alpha\nbeta gamma\ndelta\n";
    let start = body.find("gamma").unwrap_or(0);
    assert_eq!(expand_to_lines(body, start, start + 5), (17, 17));
    assert_eq!(expand_to_lines(body, 6, 16), (6, 17));
    assert_eq!(expand_to_lines(body, 6, 10), (6, 6));
    assert_eq!(expand_to_lines(body, 0, body.len()), (0, body.len()));
}

#[test]
fn headings_and_frontmatter_do_not_count_as_substance() {
    let body = format!("---\ntitle: a long frontmatter title that is not guidance at all, really, none\n---\n# A heading that is long enough to otherwise matter a lot here\n{LONG}");
    assert!(substantive_chars(&body, 0, body.len()) > 80);
    let only_front = "---\ntitle: a long frontmatter title that is not guidance at all, really, none, truly\n---\n# Heading only here\n";
    assert_eq!(substantive_chars(only_front, 0, only_front.len()), 0);
}

#[test]
fn only_uncovered_ranges_with_more_than_eighty_characters_are_worth_splitting() {
    let body = format!("# H\nshort line\ncovered middle line stays\n{LONG}tail\n");
    let short_at = body.find("short").unwrap_or(0);
    let long_at = body.find("This is").unwrap_or(0);
    let ranges = worth_splitting(&body, &[(short_at, short_at + 10), (long_at, long_at + LONG.len() - 1)]);
    assert_eq!(ranges, vec![(long_at, long_at + LONG.len())]);
    assert!(worth_splitting(&body, &[(short_at, short_at + 10)]).is_empty());
}

#[test]
fn neighbouring_ranges_merge_into_one() {
    let body = format!("{LONG}{LONG}");
    let ranges = worth_splitting(&body, &[(0, LONG.len() - 1), (LONG.len(), 2 * LONG.len() - 1)]);
    assert_eq!(ranges, vec![(0, body.len())]);
}
