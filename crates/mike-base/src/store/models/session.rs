use chrono::{DateTime, Utc};

#[cfg(test)]
use super::intercepted_request::InterceptedRequest;

#[derive(Debug, Clone)]
pub struct SessionView {
    pub id: String,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub request_count: usize,
    pub models: Vec<String>,
    pub last_status: String,
    pub pinned: bool,
    pub ttl_override: Option<String>,
    pub title: Option<String>,
}

#[cfg(any(test, feature = "test-seams"))]
#[derive(Debug, Clone)]
#[cfg(any(test, feature = "test-seams"))]
pub struct SessionViewDetail {
    pub summary: SessionView,
    pub requests: Vec<InterceptedRequest>,
}

#[derive(Debug, Clone)]
pub struct SessionMeta {
    pub pinned: bool,
    pub ttl_override: Option<String>,
    pub title: Option<String>,
    pub session_id: Option<i64>,
}

impl SessionMeta {
    pub fn new() -> Self {
        Self {
            pinned: false,
            ttl_override: None,
            title: None,
            session_id: None,
        }
    }

    pub fn is_pinned(&self) -> bool {
        self.pinned
    }

    pub fn ttl_override(&self) -> Option<&str> {
        self.ttl_override.as_deref()
    }

    pub fn session_id(&self) -> Option<i64> {
        self.session_id
    }
}

impl Default for SessionMeta {
    fn default() -> Self {
        Self::new()
    }
}
