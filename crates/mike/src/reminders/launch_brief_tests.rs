use chrono::Utc;
use uuid::Uuid;

use super::brief_reminder;
use crate::store::models::ReminderSource;
use crate::store::persistence::workspace::QueuedLaunchBrief;

fn queued(body: &str) -> QueuedLaunchBrief {
    QueuedLaunchBrief {
        message_uuid: Uuid::new_v4(),
        body: body.to_string(),
        queued_at: Utc::now(),
    }
}

#[test]
fn brief_mail_is_an_ordinary_mailbox_reminder_from_no_session() {
    let to = Uuid::new_v4();
    let brief = queued("Your task brief from mike (launch 7).\n\nFix the defects.");
    let reminder = brief_reminder(to, &brief);
    assert_eq!(
        reminder.source,
        ReminderSource::SessionMailbox {
            message_uuid: brief.message_uuid,
            from_session_uuid: None,
        }
    );
    assert_eq!(
        reminder.payload,
        "<system-reminder>\nMail from mike: Your task brief from mike (launch 7).\n\nFix the defects.\n</system-reminder>"
    );
}
