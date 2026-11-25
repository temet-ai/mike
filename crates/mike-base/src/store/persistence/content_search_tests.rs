use super::{ContentQuery, ContentQueryError, MAX_QUERY_WORDS, SearchMode};

#[test]
fn phrase_query_keeps_the_whole_text_as_one_pattern() {
    let query = ContentQuery::phrase("Executor install");

    assert_eq!(query.mode(), SearchMode::Phrase);
    assert_eq!(query.words(), ["Executor install"]);
    assert_eq!(query.patterns, ["%executor install%"]);
    assert_eq!(query.min_words, 1);
}

#[test]
fn all_mode_requires_every_distinct_word_ignoring_case_and_repeats() -> anyhow::Result<()> {
    let query = ContentQuery::new("Executor  install executor", SearchMode::All)?;

    assert_eq!(query.words(), ["Executor", "install"]);
    assert_eq!(query.patterns, ["%executor%", "%install%"]);
    assert_eq!(query.min_words, 2);
    Ok(())
}

#[test]
fn any_mode_requires_one_word() -> anyhow::Result<()> {
    let query = ContentQuery::new("alpha beta gamma", SearchMode::Any)?;

    assert_eq!(query.patterns.len(), 3);
    assert_eq!(query.min_words, 1);
    Ok(())
}

#[test]
fn word_modes_reject_a_query_without_words() {
    let error = ContentQuery::new("   ", SearchMode::All).err();

    assert_eq!(error, Some(ContentQueryError::NoWords));
}

#[test]
fn word_modes_reject_more_than_the_word_cap() {
    let text = (0..=MAX_QUERY_WORDS)
        .map(|n| format!("w{n}"))
        .collect::<Vec<_>>()
        .join(" ");

    let error = ContentQuery::new(&text, SearchMode::Any).err();

    assert_eq!(
        error,
        Some(ContentQueryError::TooManyWords {
            count: MAX_QUERY_WORDS + 1,
            max: MAX_QUERY_WORDS
        })
    );
}

#[test]
fn search_mode_parses_the_three_names_and_nothing_else() {
    assert_eq!(SearchMode::parse("phrase"), Some(SearchMode::Phrase));
    assert_eq!(SearchMode::parse("all"), Some(SearchMode::All));
    assert_eq!(SearchMode::parse("any"), Some(SearchMode::Any));
    assert_eq!(SearchMode::parse("ALL"), None);
}

#[test]
fn patterns_fold_each_letter_to_lower_case() {
    let query = ContentQuery::phrase("ÉCOLE İ ΣΑΣ");

    assert_eq!(query.patterns, ["%école i σασ%"]);
}

#[test]
fn a_word_shorter_than_a_trigram_scans_every_chunk() -> anyhow::Result<()> {
    assert!(ContentQuery::new("find a name", SearchMode::Any)?.scan_all);
    assert!(!ContentQuery::new("find the name", SearchMode::Any)?.scan_all);
    Ok(())
}

#[test]
fn wildcards_and_non_ascii_words_are_not_literal() {
    assert!(ContentQuery::phrase("plain words").matches_are_literal());
    for q in ["session_id", "100%", "back\\slash", "école"] {
        assert!(!ContentQuery::phrase(q).matches_are_literal(), "{q}");
    }
}
