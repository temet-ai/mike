const REMINDER_OPEN: &str = "<system-reminder>";
const REMINDER_CLOSE: &str = "</system-reminder>";

pub(super) fn reminder_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = text[from..].find(REMINDER_OPEN) {
        let start = from + rel;
        let body_start = start + REMINDER_OPEN.len();
        let end = text[body_start..]
            .find(REMINDER_CLOSE)
            .map_or(text.len(), |close| {
                body_start + close + REMINDER_CLOSE.len()
            });
        spans.push((start, end));
        from = end;
    }
    spans
}

pub(super) fn offset_in_spans(spans: &[(usize, usize)], offset: usize) -> bool {
    spans
        .iter()
        .any(|&(start, end)| offset >= start && offset < end)
}

#[cfg(test)]
#[path = "search_reminders_tests.rs"]
mod search_reminders_tests;
