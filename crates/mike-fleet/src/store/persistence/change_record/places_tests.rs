use super::PlaceMemo;

#[test]
fn place_memo_unknown_session_misses() {
    let memo = PlaceMemo::default();
    memo.remember(1, "/work/a", 10);
    assert_eq!(memo.get(2, "/work/a"), None);
    assert_eq!(memo.get(1, "/work/b"), None);
}

#[test]
fn place_memo_remembered_place_is_served() {
    let memo = PlaceMemo::default();
    memo.remember(1, "/work/a", 10);
    assert_eq!(memo.get(1, "/work/a"), Some(10));
}

#[test]
fn place_memo_older_place_never_moves_it_back() {
    let memo = PlaceMemo::default();
    memo.remember(1, "/work/a", 10);
    memo.remember(1, "/work/a", 4);
    assert_eq!(memo.get(1, "/work/a"), Some(10));
    memo.remember(1, "/work/a", 12);
    assert_eq!(memo.get(1, "/work/a"), Some(12));
}
