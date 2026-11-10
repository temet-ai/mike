use super::*;

#[test]
fn reminder_spans_closed_tag_covers_open_through_close() {
    let text = "a <system-reminder>x needle y</system-reminder> b needle";
    let spans = reminder_spans(text);
    assert_eq!(spans.len(), 1);
    let inside = text.find("needle").unwrap_or(usize::MAX);
    let outside = text.rfind("needle").unwrap_or(usize::MAX);
    assert!(offset_in_spans(&spans, inside));
    assert!(!offset_in_spans(&spans, outside));
}

#[test]
fn reminder_spans_unclosed_tag_runs_to_end_of_text() {
    let text = "start <system-reminder>never closed needle";
    let spans = reminder_spans(text);
    assert_eq!(spans, vec![(6, text.len())]);
}

#[test]
fn reminder_spans_without_tags_is_empty() {
    assert!(reminder_spans("plain needle text").is_empty());
}

#[test]
fn reminder_spans_two_blocks_are_separate_spans() {
    let text = "<system-reminder>a</system-reminder> mid <system-reminder>b</system-reminder>";
    assert_eq!(reminder_spans(text).len(), 2);
    assert!(!offset_in_spans(&reminder_spans(text), 40));
}
