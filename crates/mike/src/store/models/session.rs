use chrono::{DateTime, Utc};

#[cfg(test)]
use super::intercepted_request::InterceptedRequest;

#[derive(Debug, Clone)]
pub(crate) struct SessionView {
    pub(crate) id: String,
    pub(crate) first_seen: DateTime<Utc>,
    pub(crate) last_seen: DateTime<Utc>,
    pub(crate) request_count: usize,
    pub(crate) models: Vec<String>,
    pub(crate) last_status: String,
    pub(crate) pinned: bool,
    pub(crate) ttl_override: Option<String>,
    pub(crate) title: Option<String>,
}

#[cfg(any(test, feature = "test-seams"))]
#[derive(Debug, Clone)]
#[cfg(test)]
pub(crate) struct SessionViewDetail {
    pub(crate) summary: SessionView,
    pub(crate) requests: Vec<InterceptedRequest>,
}

#[derive(Debug, Clone)]
pub(crate) struct SessionMeta {
    pub(crate) pinned: bool,
    pub(crate) ttl_override: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) session_id: Option<i64>,
}

impl SessionMeta {
    pub(crate) fn new() -> Self {
        Self {
            pinned: false,
            ttl_override: None,
            title: None,
            session_id: None,
        }
    }

    pub(crate) fn is_pinned(&self) -> bool {
        self.pinned
    }

    pub(crate) fn ttl_override(&self) -> Option<&str> {
        self.ttl_override.as_deref()
    }

    pub(crate) fn session_id(&self) -> Option<i64> {
        self.session_id
    }
}

impl Default for SessionMeta {
    fn default() -> Self {
        Self::new()
    }
}
