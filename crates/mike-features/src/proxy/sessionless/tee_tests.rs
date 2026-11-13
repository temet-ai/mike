use super::tee::Kept;

#[test]
fn kept_body_uncapped_is_kept_whole() {
    let mut kept = Kept::default();
    kept.take(b"abc");
    kept.take(b"def");
    assert_eq!(&kept.bytes[..], b"abcdef");
    assert_eq!(kept.size, 6);
    assert!(!kept.refused);
}

#[test]
fn kept_body_over_the_cap_is_refused_whole_and_every_byte_counted() {
    let mut kept = Kept {
        cap: 4,
        ..Kept::default()
    };
    kept.take(b"abc");
    kept.take(b"def");
    kept.take(b"ghi");
    assert!(kept.bytes.is_empty(), "no shortened prefix is kept");
    assert_eq!(kept.size, 9);
    assert!(kept.refused);
}

#[test]
fn kept_body_exactly_at_the_cap_is_kept() {
    let mut kept = Kept {
        cap: 3,
        ..Kept::default()
    };
    kept.take(b"abc");
    assert!(!kept.refused);
    assert_eq!(&kept.bytes[..], b"abc");
}
