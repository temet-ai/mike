use chrono::{DateTime, Duration, Utc};

use super::corpus_span;

fn at(secs: i64) -> anyhow::Result<DateTime<Utc>> {
    DateTime::from_timestamp(secs, 0).ok_or_else(|| anyhow::anyhow!("bad timestamp"))
}

#[test]
fn corpus_span_within_two_days_is_complete() -> anyhow::Result<()> {
    let to = at(1_800_000_000)?;
    let from = to - Duration::days(1);
    let span = corpus_span(from, to, None);
    assert_eq!((span.lower, span.upper, span.next_cursor), (from, to, None));
    Ok(())
}

#[test]
fn corpus_span_over_a_wide_window_reads_the_newest_two_days_and_resumes() -> anyhow::Result<()> {
    let to = at(1_800_000_000)?;
    let from = to - Duration::days(5);
    let first = corpus_span(from, to, None);
    assert_eq!(first.upper, to);
    assert_eq!(first.lower, to - Duration::days(2));
    let cursor = first.next_cursor.ok_or_else(|| anyhow::anyhow!("a cursor"))?;
    let second = corpus_span(from, to, Some(cursor));
    assert_eq!(second.upper, first.lower);
    assert_eq!(second.lower, to - Duration::days(4));
    let third = corpus_span(from, to, second.next_cursor);
    assert_eq!((third.lower, third.next_cursor), (from, None));
    Ok(())
}

#[test]
fn corpus_span_with_a_cursor_past_the_window_end_starts_at_the_end() -> anyhow::Result<()> {
    let to = at(1_800_000_000)?;
    let from = to - Duration::days(1);
    let span = corpus_span(from, to, Some(1_900_000_000));
    assert_eq!((span.upper, span.next_cursor), (to, None));
    Ok(())
}
