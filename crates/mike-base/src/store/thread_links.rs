use super::RequestStore;
use super::models::{InterceptedRequest, RequestId, ThreadLink, is_fallback};

pub const MAX_FOLLOWUPS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FollowupAnswer {
    NotFollowedUp,
    NotHeld,
    EndsAt(String),
    Unanswered,
}

impl RequestStore {
    pub fn followup_answer(&self, session_id: &str, previous: &str) -> FollowupAnswer {
        let Some(start) = self.request_answered_by(session_id, previous) else {
            return FollowupAnswer::NotHeld;
        };
        let mut current = start;
        for _ in 0..MAX_FOLLOWUPS {
            let Some(next) = current.correction_request_id else {
                break;
            };
            let Some(followup) = self.entries.get(&RequestId::from_raw(next)) else {
                return FollowupAnswer::NotHeld;
            };
            current = followup;
        }
        if std::ptr::eq(current, start) {
            return FollowupAnswer::NotFollowedUp;
        }
        match current.upstream_message_id.as_deref() {
            Some(id) if !id.is_empty() => FollowupAnswer::EndsAt(id.to_owned()),
            _ => FollowupAnswer::Unanswered,
        }
    }

    pub fn link_thread(&self, req: &InterceptedRequest) -> Option<ThreadLink> {
        self.link_body(&req.session_id, req.request_body.as_deref()?)
    }

    pub fn link_body(&self, session_id: &str, body: &[u8]) -> Option<ThreadLink> {
        let mut link = ThreadLink::from_body(body)?;
        if !link.continues() {
            return Some(link);
        }
        let Some(previous) = link.previous_message_id.as_deref() else {
            return Some(link);
        };
        if let Some(found) = self.request_answered_by(session_id, previous) {
            link.predecessor = Some(found.id);
            link.predecessor_instance = found.agent_instance_id;
        }
        Some(link)
    }

    fn request_answered_by(
        &self,
        session_id: &str,
        upstream_message_id: &str,
    ) -> Option<&InterceptedRequest> {
        self.session_index
            .get(session_id)?
            .iter()
            .filter_map(|id| self.entries.get(id))
            .find(|candidate| {
                candidate.upstream_message_id.as_deref() == Some(upstream_message_id)
                    && !is_fallback(candidate.id.as_u64())
            })
    }
}

#[cfg(test)]
#[path = "thread_links_tests.rs"]
mod tests;
