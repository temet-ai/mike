use super::*;

#[test]
fn closed_mail_notice_identifies_attempt_without_body_or_reroute_claims() {
    let (sender, recipient, coordinator) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let attempt = ClosedMailAttempt {
        id: 17,
        sender,
        recipient,
        seat: Some("worker-P1".into()),
        attempted_at: Utc::now(),
    };
    let notice = closed_mail(attempt, coordinator);
    assert_eq!(notice.to, coordinator);
    assert_eq!(notice.key, "closed-mail:17");
    assert!(notice.body.contains(&sender.to_string()));
    assert!(notice.body.contains(&recipient.to_string()));
    assert!(notice.body.contains("worker-P1"));
    assert!(notice.body.contains("delivery was refused"));
}

#[test]
fn lookback_never_precedes_run_or_twenty_four_hours() {
    let now = Utc::now();
    assert_eq!(lookback(now, None), now - Duration::hours(24));
    assert_eq!(
        lookback(now, Some(now - Duration::hours(2))),
        now - Duration::hours(2)
    );
    assert_eq!(
        lookback(now, Some(now - Duration::hours(48))),
        now - Duration::hours(24)
    );
}
