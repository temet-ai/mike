use super::MAX_PENDING_REMINDERS_PER_SESSION;
use super::RequestStore;
use super::models;
use super::models::RequestId;

impl RequestStore {
    pub fn enqueue_reminder(&mut self, session_id: &str, reminder: models::SystemReminder) {
        let dq = self
            .session_pending_reminders
            .entry(session_id.to_owned())
            .or_default();
        if dq.len() >= MAX_PENDING_REMINDERS_PER_SESSION {
            dq.pop_front();
        }
        dq.push_back(reminder);
    }

    pub fn record_latest_findings(
        &mut self,
        session_id: &str,
        request_id: RequestId,
        findings: &[models::Diagnostic],
    ) {
        if findings.is_empty() {
            self.session_latest_findings.remove(session_id);
        } else {
            self.session_latest_findings
                .insert(session_id.to_owned(), (request_id, findings.to_vec()));
        }
    }

    pub fn latest_findings(
        &self,
        session_id: &str,
    ) -> Option<(RequestId, Vec<models::Diagnostic>)> {
        self.session_latest_findings.get(session_id).cloned()
    }

    pub fn claim_reminders(&mut self, session_id: &str) -> Vec<models::SystemReminder> {
        self.session_pending_reminders
            .remove(session_id)
            .map(|dq| dq.into_iter().collect())
            .unwrap_or_default()
    }

    pub fn peek_reminders(&self, session_id: &str) -> Vec<models::SystemReminder> {
        self.session_pending_reminders
            .get(session_id)
            .map(|dq| dq.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn requeue_reminder(&mut self, session_id: &str, reminder: models::SystemReminder) {
        self.session_pending_reminders
            .entry(session_id.to_owned())
            .or_default()
            .push_front(reminder);
    }

    pub fn sessions_with_pending_reminders(&self) -> Vec<String> {
        self.session_pending_reminders
            .iter()
            .filter(|(_, dq)| !dq.is_empty())
            .map(|(session_id, _)| session_id.clone())
            .collect()
    }

    pub fn take_reminders_where(
        &mut self,
        session_id: &str,
        mut take: impl FnMut(&models::SystemReminder) -> bool,
    ) -> Vec<models::SystemReminder> {
        let Some(dq) = self.session_pending_reminders.get_mut(session_id) else {
            return Vec::new();
        };
        let (taken, kept): (Vec<_>, Vec<_>) = dq.drain(..).partition(|r| take(r));
        dq.extend(kept);
        taken
    }

    pub fn remove_mailbox_reminder(&mut self, session_id: &str, message_uuid: uuid::Uuid) {
        if let Some(dq) = self.session_pending_reminders.get_mut(session_id) {
            dq.retain(|r| {
                !matches!(
                    &r.source,
                    models::ReminderSource::SessionMailbox { message_uuid: m, .. }
                        if *m == message_uuid
                )
            });
        }
    }
}
