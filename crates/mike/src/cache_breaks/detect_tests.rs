use super::*;
use chrono::{Duration, TimeZone};

fn usage(cache_read: i64, cache_creation: i64, creation_1h: i64) -> Usage {
    Usage {
        cache_read,
        cache_creation,
        input: 2,
        creation_1h,
        creation_5m: cache_creation - creation_1h,
    }
}

fn candidate(gap_secs: i64, prev: Usage, cur: Usage) -> Candidate {
    let done = Utc
        .with_ymd_and_hms(2026, 11, 17, 12, 0, 0)
        .single()
        .unwrap_or_else(Utc::now);
    Candidate {
        request_id: 20,
        session_id: 1,
        model: Some("claude-opus-5-5".to_owned()),
        created_at: done + Duration::seconds(gap_secs),
        woken_by: "operator".to_owned(),
        usage: cur,
        prev_request_id: 10,
        prev_model: Some("claude-opus-5-5".to_owned()),
        prev_done_at: done,
        prev_usage: prev,
    }
}

#[test]
fn judge_warm_resend_that_reads_little_is_a_break() {
    let c = candidate(
        120,
        usage(500_000, 4_000, 4_000),
        usage(29_000, 515_000, 515_000),
    );
    let v = judge(&c).map(|v| (v.kind, v.ttl_seconds, v.expected_read, v.tokens_rewritten));
    assert_eq!(v, Some((Kind::Break, 3_600, 504_000, 515_000)));
}

#[test]
fn judge_resend_that_reads_the_prefix_is_no_break() {
    let c = candidate(
        120,
        usage(500_000, 4_000, 4_000),
        usage(504_000, 6_000, 6_000),
    );
    assert_eq!(judge(&c), None);
}

#[test]
fn judge_resume_after_the_hour_that_rewrites_is_an_expiry() {
    let c = candidate(
        4_000,
        usage(500_000, 4_000, 4_000),
        usage(0, 504_000, 504_000),
    );
    assert_eq!(judge(&c).map(|v| v.kind), Some(Kind::Expiry));
}

#[test]
fn judge_five_minute_cache_expires_after_five_minutes() {
    let c = candidate(400, usage(50_000, 8_000, 0), usage(0, 58_000, 0));
    let v = judge(&c).map(|v| (v.kind, v.ttl_seconds));
    assert_eq!(v, Some((Kind::Expiry, 300)));
}

#[test]
fn judge_model_change_is_never_a_break() {
    let mut c = candidate(60, usage(500_000, 4_000, 4_000), usage(0, 504_000, 504_000));
    c.prev_model = Some("claude-sonnet-5".to_owned());
    assert_eq!(judge(&c), None);
}

#[test]
fn judge_small_rewrite_is_ignored() {
    let c = candidate(60, usage(500_000, 4_000, 4_000), usage(0, 4_500, 4_500));
    assert_eq!(judge(&c), None);
}
