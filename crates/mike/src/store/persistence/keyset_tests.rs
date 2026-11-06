use chrono::DateTime;

use super::{COUNT_CAP, CappedCount, KeysetCursor, probe_limit, split_page};

#[test]
fn cursor_round_trips_through_its_encoding() -> anyhow::Result<()> {
    let at = DateTime::from_timestamp_micros(1_761_596_336_615_173)
        .ok_or_else(|| anyhow::anyhow!("bad micros"))?;
    let cursor = KeysetCursor { at, id: 1_222_272 };
    assert_eq!(KeysetCursor::decode(&cursor.encode()), Some(cursor));
    Ok(())
}

#[test]
fn cursor_decode_rejects_malformed_input() {
    assert_eq!(KeysetCursor::decode(""), None);
    assert_eq!(KeysetCursor::decode("12345"), None);
    assert_eq!(KeysetCursor::decode("abc.12"), None);
    assert_eq!(KeysetCursor::decode("12.abc"), None);
}

#[test]
fn capped_count_flags_a_probe_past_the_cap() {
    assert_eq!(
        CappedCount::from_probe(COUNT_CAP + 1),
        CappedCount {
            count: COUNT_CAP,
            capped: true
        }
    );
    assert_eq!(
        CappedCount::from_probe(12),
        CappedCount {
            count: 12,
            capped: false
        }
    );
}

#[test]
fn split_page_returns_a_cursor_only_when_the_probe_row_came_back() -> anyhow::Result<()> {
    let at = DateTime::from_timestamp_micros(1_000).ok_or_else(|| anyhow::anyhow!("bad micros"))?;
    let key = |id: &i64| KeysetCursor { at, id: *id };
    let (rows, next) = split_page(vec![5, 4, 3], 2, key);
    assert_eq!(rows, vec![5, 4]);
    assert_eq!(next, Some(KeysetCursor { at, id: 4 }));
    let (rows, next) = split_page(vec![5, 4], 2, key);
    assert_eq!(rows, vec![5, 4]);
    assert_eq!(next, None);
    assert_eq!(probe_limit(2), 3);
    Ok(())
}
