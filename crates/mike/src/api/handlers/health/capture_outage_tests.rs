use chrono::{Duration, TimeZone, Utc};

use super::{CaptureOutageEvent, CaptureOutageTracker};

#[test]
fn outage_notice_fires_once_then_recovery_once() -> anyhow::Result<()> {
    let start = Utc
        .with_ymd_and_hms(2026, 11, 5, 10, 0, 0)
        .single()
        .ok_or_else(|| anyhow::anyhow!("bad date"))?;
    let mut tracker = CaptureOutageTracker::default();
    let mut events = Vec::new();

    for minute in 0..30 {
        let now = start + Duration::minutes(minute);
        events.extend(tracker.observe(Some("database unreachable"), now));
    }
    let after_outage = events.len();
    for minute in 30..40 {
        events.extend(tracker.observe(None, start + Duration::minutes(minute)));
    }

    assert_eq!(after_outage, 1, "a long outage notifies once: {events:?}");
    assert_eq!(
        events.first(),
        Some(&CaptureOutageEvent::Down {
            since: start,
            reason: "database unreachable".to_owned()
        })
    );
    assert_eq!(
        events.get(1),
        Some(&CaptureOutageEvent::Recovered { since: start })
    );
    assert_eq!(events.len(), 2);
    Ok(())
}

#[test]
fn single_failed_reading_stays_silent() {
    let mut tracker = CaptureOutageTracker::default();
    let now = Utc::now();

    assert_eq!(tracker.observe(Some("timeout"), now), None);
    assert_eq!(tracker.observe(None, now), None);
}

#[test]
fn down_message_says_nothing_is_refused_and_names_the_doc() {
    let event = CaptureOutageEvent::Down {
        since: Utc::now(),
        reason: "disk full".to_owned(),
    };

    let message = event.message();

    assert!(message.contains("no request is refused"), "{message}");
    assert!(message.contains("disk full"), "{message}");
    assert!(message.contains("docs/CAPTURE_OUTAGES.md"), "{message}");
}
