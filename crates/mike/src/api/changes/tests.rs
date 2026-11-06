use super::{ChangesError, parse_scope, parse_since, parse_view};
use crate::change_record::answers::{Scope, Since};

#[test]
fn since_defaults_to_the_place_and_reads_times_and_commit_ids() -> anyhow::Result<()> {
    assert_eq!(parse_since(None)?, Since::Place);
    assert_eq!(parse_since(Some("place"))?, Since::Place);
    assert!(matches!(
        parse_since(Some("2026-10-05T12:00:00Z"))?,
        Since::Time(_)
    ));
    assert_eq!(
        parse_since(Some("ABCDEF12"))?,
        Since::Commit("abcdef12".to_owned())
    );
    Ok(())
}

#[test]
fn since_with_neither_time_nor_commit_names_the_input_in_the_error() {
    let error = parse_since(Some("yesterday-ish"));
    let Err(ChangesError::Invalid(message)) = error else {
        panic!("expected an invalid-request error");
    };
    assert!(message.contains("yesterday-ish"), "{message}");
}

#[test]
fn scope_is_mine_by_default_paths_when_listed_and_all_on_request() -> anyhow::Result<()> {
    assert_eq!(parse_scope(None, None)?, Scope::Mine);
    assert_eq!(parse_scope(Some("all"), None)?, Scope::All);
    assert_eq!(
        parse_scope(None, Some("a.rs, b.rs"))?,
        Scope::Paths(vec!["a.rs".to_owned(), "b.rs".to_owned()])
    );
    assert!(parse_scope(Some("paths"), None).is_err());
    assert!(parse_scope(Some("everything"), None).is_err());
    Ok(())
}

#[test]
fn view_accepts_diff_and_rejects_unknown_values_with_the_input() {
    for view in [
        "changes", "standing", "whose", "mine", "diff", "tail", "feed",
    ] {
        assert!(parse_view(Some(view)).is_ok(), "{view}");
    }
    let Err(ChangesError::Invalid(message)) = parse_view(Some("unknown-view")) else {
        panic!("expected an invalid-request error");
    };
    assert!(message.contains("unknown-view"), "{message}");
    assert!(parse_view(None).is_err());
}
