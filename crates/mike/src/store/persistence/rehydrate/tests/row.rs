use crate::store::models::RequestStatus;
use crate::store::persistence::rehydrate::row::parse_status;

#[test]
fn parse_status_handles_known_variants() {
    assert!(matches!(parse_status("Pending"), RequestStatus::Pending));
    assert!(matches!(
        parse_status("Streaming"),
        RequestStatus::Streaming
    ));
    assert!(matches!(parse_status("Complete"), RequestStatus::Complete));
    assert!(matches!(parse_status("pending"), RequestStatus::Pending));
    assert!(matches!(
        parse_status("streaming"),
        RequestStatus::Streaming
    ));
    assert!(matches!(parse_status("complete"), RequestStatus::Complete));
}

#[test]
fn parse_status_handles_error_with_message() {
    match parse_status("Error: overloaded_error") {
        RequestStatus::Error(msg) => assert_eq!(msg, "overloaded_error"),
        other => panic!("expected Error variant, got {other:?}"),
    }
}

#[test]
fn parse_status_falls_through_unknown_to_error() {
    match parse_status("ToDo-unknown") {
        RequestStatus::Error(msg) => assert_eq!(msg, "ToDo-unknown"),
        other => panic!("expected Error fallthrough, got {other:?}"),
    }
}
