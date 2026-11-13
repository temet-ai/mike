use super::*;

#[test]
fn kind_round_trips_through_strings() {
    for kind in [
        FilterKind::TextSubstring,
        FilterKind::XmlTag,
        FilterKind::LazySection,
        FilterKind::InjectReminder,
    ] {
        let s = kind_to_str(kind);
        assert_eq!(str_to_kind(s), Some(kind));
    }
}

#[test]
fn action_round_trips_through_strings() {
    for action in [
        FilterAction::Drop,
        FilterAction::Replace,
        FilterAction::Append,
    ] {
        let s = action_to_str(action);
        assert_eq!(str_to_action(s), Some(action));
    }
}

#[test]
fn source_round_trips_through_strings() {
    for source in [RuleSource::Toml, RuleSource::Ui] {
        let s = source_to_str(source);
        assert_eq!(str_to_source(s), Some(source));
    }
}

#[test]
fn unknown_strings_yield_none() {
    assert!(str_to_kind("bogus").is_none());
    assert!(str_to_action("bogus").is_none());
    assert!(str_to_source("bogus").is_none());
}
