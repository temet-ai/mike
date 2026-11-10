use super::short_query::too_short;
use super::{RerankReport, RerankState};

#[test]
fn too_short_with_fewer_words_than_wanted_reports_off_with_the_reason() {
    let report = too_short("cache keys", 3);

    assert_eq!(report.as_ref().map(|r| r.state), Some(RerankState::Off));
    assert_eq!(
        report.and_then(|r| r.detail).as_deref(),
        Some(
            "the query has fewer than 3 words, and short keyword queries keep plain order; these results are in plain order"
        )
    );
}

#[test]
fn too_short_with_as_many_words_as_wanted_is_none() {
    assert_eq!(too_short("cache keys expire", 3), None::<RerankReport>);
}

#[test]
fn too_short_with_the_default_of_one_word_never_applies() {
    assert_eq!(too_short("cache", 1), None::<RerankReport>);
    assert_eq!(too_short("", 1).map(|r| r.state), Some(RerankState::Off));
}

#[test]
fn too_short_counts_words_split_on_any_whitespace() {
    assert_eq!(
        too_short("  cache \t keys\n expire ", 3),
        None::<RerankReport>
    );
    assert!(too_short("  cache \t keys  ", 3).is_some());
}
