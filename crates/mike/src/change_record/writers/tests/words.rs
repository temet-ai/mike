use crate::change_record::writers::words::{commit_candidates, path_candidates};

#[test]
fn commit_candidates_hex_words_with_digits_and_letters_are_found_once() {
    let found = commit_candidates(
        "see 48bfe2e4, and `48bfe2e4` plus deadbeef12 but not 1234567 or cafebabe",
    );
    assert_eq!(
        found,
        vec!["48bfe2e4".to_string(), "deadbeef12".to_string()]
    );
}

#[test]
fn commit_candidates_uppercase_and_short_words_are_ignored() {
    assert!(commit_candidates("ABCDEF123 abc12 plain words").is_empty());
}

#[test]
fn path_candidates_strip_punctuation_line_numbers_and_the_checkout_prefix() {
    let found = path_candidates(
        "edit `src/lib.rs:12:3`, (./docs/a.md) and /work/x/crates/m.rs. nothing here",
        "/work/x",
    );
    assert_eq!(
        found,
        vec![
            "src/lib.rs".to_string(),
            "docs/a.md".to_string(),
            "crates/m.rs".to_string()
        ]
    );
}

#[test]
fn path_candidates_words_without_a_slash_are_not_paths() {
    assert!(path_candidates("README.md Cargo.toml", "/work/x").is_empty());
}
