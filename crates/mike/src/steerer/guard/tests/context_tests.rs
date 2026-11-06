use super::turn_ending;
use crate::store::persistence::dossier_view::OpenTextRow;

fn row(request_id: i64, origin: &str, content: &str) -> OpenTextRow {
    OpenTextRow {
        request_id,
        seq_ord: 0,
        origin: origin.to_owned(),
        content: content.to_owned(),
    }
}

#[test]
fn the_live_reply_ends_the_turn_when_its_text_is_not_stored_yet() {
    let rows = vec![row(1, "operator", "Let me check what's installed.")];
    let ending = turn_ending(&rows, Some(3), "You already have Kdenlive and ffmpeg.");
    assert!(ending.ends_with("You already have Kdenlive and ffmpeg."));
}

#[test]
fn the_live_reply_is_not_counted_twice_once_it_is_stored() {
    let rows = vec![
        row(1, "operator", "Let me check. "),
        row(3, "operator", "All done."),
    ];
    assert_eq!(
        turn_ending(&rows, Some(3), "All done."),
        "Let me check. All done."
    );
}

#[test]
fn stored_text_stands_when_there_is_no_live_reply() {
    let rows = vec![
        row(1, "operator", "Tests pass."),
        row(2, "sidecar", "title"),
    ];
    assert_eq!(turn_ending(&rows, None, ""), "Tests pass.");
    assert_eq!(turn_ending(&rows, Some(2), ""), "Tests pass.");
}

#[test]
fn the_live_text_stands_in_when_nothing_is_stored() {
    assert_eq!(turn_ending(&[], None, "tail"), "tail");
}
