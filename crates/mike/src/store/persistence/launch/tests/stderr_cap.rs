use crate::store::persistence::launch::{STDERR_CAP_BYTES, StderrCapture};

#[test]
fn capture_under_cap_keeps_everything_unflagged() {
    let raw = "herdr: connection refused\n";
    let got = StderrCapture::capture(raw);
    assert_eq!(got.text, raw);
    assert!(!got.truncated);
}

#[test]
fn capture_exactly_at_cap_is_not_truncated() {
    let raw = "x".repeat(STDERR_CAP_BYTES);
    let got = StderrCapture::capture(&raw);
    assert_eq!(got.text.len(), STDERR_CAP_BYTES);
    assert!(!got.truncated);
}

#[test]
fn capture_over_cap_keeps_full_prefix_and_sets_flag() {
    let raw = format!("{}TAIL", "y".repeat(STDERR_CAP_BYTES));
    let got = StderrCapture::capture(&raw);
    assert_eq!(got.text.len(), STDERR_CAP_BYTES);
    assert_eq!(got.text, &raw[..STDERR_CAP_BYTES]);
    assert!(got.truncated);
}

#[test]
fn capture_over_cap_never_splits_a_multibyte_char() {
    let raw = "☃".repeat(STDERR_CAP_BYTES / 3 + 10);
    let got = StderrCapture::capture(&raw);
    assert!(got.truncated);
    assert!(got.text.len() <= STDERR_CAP_BYTES);
    assert_eq!(got.text.len(), 3 * (STDERR_CAP_BYTES / 3));
    assert!(got.text.chars().all(|c| c == '☃'));
}

#[test]
fn from_bytes_invalid_utf8_is_replaced_not_rejected() {
    let got = StderrCapture::from_bytes(b"ok \xff\xfe bad");
    assert_eq!(got.text, "ok \u{FFFD}\u{FFFD} bad");
    assert!(!got.truncated);
}
