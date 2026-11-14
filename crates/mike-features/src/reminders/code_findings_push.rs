use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use mike_base::code_notes::{CodeNote, CodeNotes};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::feature_toggles::FeatureToggles;
use crate::proxy::remote_control::inject::{PushDeps, try_push_reminder};
use crate::store::models::{ReminderSource, SystemReminder};

const TICK: Duration = Duration::from_secs(2);
const UNREAD_FOR: Duration = Duration::from_secs(8);
const PER_PUSH: usize = 5;

pub fn push_text(notes: &[CodeNote]) -> String {
    super::code_findings::note_text(notes)
}

pub struct Pusher {
    pub deps: PushDeps,
    pub notes: Arc<CodeNotes>,
    pub toggles: Arc<ArcSwap<FeatureToggles>>,
}

impl Pusher {
    pub fn spawn(self, tracker: &tokio_util::task::TaskTracker, shutdown: CancellationToken) {
        tracker.spawn(self.run(shutdown));
    }

    async fn run(self, shutdown: CancellationToken) {
        loop {
            tokio::select! {
                () = shutdown.cancelled() => return,
                () = tokio::time::sleep(TICK) => {}
            }
            let current = self.toggles.load();
            if current.code_findings && current.code_scan && current.code_findings_push {
                self.push_unread(UNREAD_FOR).await;
            }
        }
    }

    pub async fn push_unread(&self, unread_for: Duration) -> usize {
        let mut pushed = 0;
        for (session, notes) in self.notes.take_unread(unread_for, PER_PUSH) {
            if self.push(session, notes).await {
                pushed += 1;
            }
        }
        pushed
    }

    async fn push(&self, session: Uuid, notes: Vec<CodeNote>) -> bool {
        let reminder = SystemReminder {
            source: ReminderSource::CodeFindings,
            payload: push_text(&notes),
        };
        let ids: Vec<i64> = notes.iter().map(|n| n.id).collect();
        if try_push_reminder(&self.deps, session, &reminder).await {
            self.notes.mark_pushed(session, &ids);
            tracing::info!(session_id = %session, findings = ids.len(), "code findings: pushed to an idle session over Remote Control");
            return true;
        }
        self.notes.put_back_unpushable(session, notes);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(id: i64) -> CodeNote {
        CodeNote {
            id,
            path: "scripts/x.sh".to_owned(),
            line: 4,
            rule_id: "sh-fixed-tmp-path".to_owned(),
            message: "A fixed path in the shared temp directory.".to_owned(),
            fix: Some("mktemp".to_owned()),
        }
    }

    fn pusher() -> Pusher {
        Pusher {
            deps: PushDeps {
                persist: None,
                remote_control_streams: crate::proxy::remote_control::ReplyStreams::default(),
                gate: Arc::new(crate::injection::InjectionGate::default()),
                store: Arc::new(tokio::sync::RwLock::new(crate::store::RequestStore::new(
                    crate::store::retention::RetentionPolicy::default(),
                ))),
            },
            notes: Arc::new(CodeNotes::default()),
            toggles: Arc::new(ArcSwap::from_pointee(FeatureToggles::default())),
        }
    }

    #[test]
    fn push_text_names_each_finding_and_asks_for_a_fix() {
        let text = push_text(&[note(1)]);
        assert!(text.starts_with(super::super::code_findings::HEADING));
        assert!(text.ends_with(super::super::code_findings::ASK));
        assert!(text.contains("scripts/x.sh:4 sh-fixed-tmp-path"));
        assert!(text.contains("Fix: `mktemp`"));
    }

    #[tokio::test]
    async fn push_without_a_remote_control_stream_leaves_the_note_for_the_next_request() {
        let pusher = pusher();
        let session = Uuid::new_v4();
        pusher.notes.post(session, vec![note(1)]);
        assert_eq!(pusher.push_unread(Duration::ZERO).await, 0);
        assert_eq!(pusher.push_unread(Duration::ZERO).await, 0);
        let left = pusher.notes.take(session, 3);
        assert_eq!(left.len(), 1);
        assert!(pusher.notes.drain_delivered().is_empty());
    }
}
