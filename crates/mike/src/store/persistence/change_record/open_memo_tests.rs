use std::sync::Arc;

use chrono::Utc;

use super::{OpenChangesMemo, Watermark};
use crate::change_record::OpenChange;

fn rows(path: &str) -> Arc<Vec<OpenChange>> {
    Arc::new(vec![OpenChange {
        path: path.to_owned(),
        session_id: Some(7),
        at: Utc::now(),
    }])
}

fn at(memo: &OpenChangesMemo, newest: Option<i64>) -> Watermark {
    Watermark {
        newest,
        revision: memo.revision(),
    }
}

#[test]
fn memo_serves_rows_while_the_watermark_is_unchanged() {
    let memo = OpenChangesMemo::default();
    let mark = at(&memo, Some(10));
    memo.remember("/work/a", mark, rows("a.rs"));
    let hit = memo.get("/work/a", at(&memo, Some(10)));
    assert_eq!(hit.map(|r| r[0].path.clone()).as_deref(), Some("a.rs"));
    assert!(memo.get("/work/b", at(&memo, Some(10))).is_none());
}

#[test]
fn memo_misses_once_a_newer_event_lands() {
    let memo = OpenChangesMemo::default();
    memo.remember("/work/a", at(&memo, Some(10)), rows("a.rs"));
    assert!(memo.get("/work/a", at(&memo, Some(11))).is_none());
}

#[test]
fn memo_misses_after_a_revision() {
    let memo = OpenChangesMemo::default();
    memo.remember("/work/a", at(&memo, Some(10)), rows("a.rs"));
    memo.revise();
    assert!(memo.get("/work/a", at(&memo, Some(10))).is_none());
}

#[test]
fn memo_drops_rows_computed_before_a_revision() {
    let memo = OpenChangesMemo::default();
    let stale = at(&memo, Some(10));
    memo.revise();
    memo.remember("/work/a", stale, rows("a.rs"));
    assert!(memo.get("/work/a", stale).is_none());
    assert!(memo.get("/work/a", at(&memo, Some(10))).is_none());
}
