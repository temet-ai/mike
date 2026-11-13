use super::*;
pub use crate::store::FindingView;
pub use crate::store::RequestStore;
pub use crate::store::models::{
    Diagnostic, DiagnosticCategory, InterceptedRequest, RequestId,
};
pub use crate::store::retention::RetentionPolicy;
pub use crate::store::views::FindingRequestRef;
pub use chrono::{DateTime, Duration as ChronoDuration, Utc};
pub use std::time::Duration;

pub fn make_title_sidecar_request(
    id: u64,
    session_id: &str,
    text: &str,
    created_at: DateTime<Utc>,
) -> InterceptedRequest {
    let mut req =
        make_test_request_with_timestamp(Some(RequestId::from_raw(id)), session_id, created_at);
    req.turn_origin = "title_sidecar".to_string();
    let body = serde_json::json!({"content": [{"type": "text", "text": text}]}).to_string();
    req.response_body = Some(bytes::Bytes::from(body));
    req
}

pub fn small_policy(max_entries: usize) -> RetentionPolicy {
    RetentionPolicy {
        max_entries,
        max_age: Duration::from_secs(3600),
        prune_interval: Duration::from_secs(60),
        max_store_bytes: 0,
    }
}

use crate::store::models::{SessionView, SessionViewDetail};

impl RequestStore {
    fn session_view(
        &self,
        session_id: &str,
        bucket: &std::collections::VecDeque<RequestId>,
    ) -> Option<SessionView> {
        if bucket.is_empty() {
            return None;
        }
        let front_req = self.entries.get(bucket.front()?)?;
        let last_seen = front_req.created_at;
        let last_status = front_req.status.to_string();
        let mut models: Vec<String> = Vec::new();
        let mut request_count: usize = 0;
        let mut first_seen = last_seen;
        for rid in bucket.iter().rev() {
            let Some(req) = self.entries.get(rid) else {
                continue;
            };
            request_count += 1;
            if req.created_at < first_seen {
                first_seen = req.created_at;
            }
            if let Some(model) = req.model.as_deref()
                && !models.iter().any(|e| e == model)
            {
                models.push(model.to_string());
            }
        }
        if request_count == 0 {
            return None;
        }
        let meta = self
            .session_meta
            .get(session_id)
            .cloned()
            .unwrap_or_default();
        Some(SessionView {
            id: session_id.to_string(),
            first_seen,
            last_seen,
            request_count,
            models,
            last_status,
            pinned: meta.is_pinned(),
            ttl_override: meta.ttl_override().map(str::to_string),
            title: meta.title.clone(),
        })
    }

    pub fn get_session(&self, id: &str) -> Option<SessionViewDetail> {
        let bucket = self.session_index.get(id)?;
        let summary = self.session_view(id, bucket)?;
        let mut requests: Vec<InterceptedRequest> = bucket
            .iter()
            .rev()
            .filter_map(|rid| self.entries.get(rid).cloned())
            .collect();
        requests.sort_by_key(|r| r.created_at);
        Some(SessionViewDetail { summary, requests })
    }

    pub fn list_sessions(&self) -> Vec<SessionView> {
        let mut views: Vec<SessionView> = self
            .session_index
            .iter()
            .filter_map(|(sid, bucket)| self.session_view(sid, bucket))
            .collect();
        views.sort_by_key(|v| std::cmp::Reverse(v.last_seen));
        views
    }

    pub fn findings(
        &self,
        category: Option<&DiagnosticCategory>,
        analyzer: Option<&str>,
    ) -> Vec<FindingView> {
        let mut out: Vec<FindingView> = Vec::new();
        for req in self.entries.values() {
            for diag in &req.diagnostics {
                if let Some(cat) = category
                    && &diag.category != cat
                {
                    continue;
                }
                if let Some(name) = analyzer
                    && diag.analyzer != name
                {
                    continue;
                }
                out.push(FindingView {
                    request_id: req.id,
                    session_id: req.session_id.clone(),
                    diagnostic: diag.clone(),
                    request: FindingRequestRef {
                        model: req.model.clone(),
                        status: req.status.to_string(),
                        path: req.path.clone(),
                        project: req.project.clone(),
                        human_prompt: req.human_prompt.clone(),
                        created_at: req.created_at,
                    },
                });
            }
        }
        out.sort_by_key(|f| std::cmp::Reverse(f.diagnostic.created_at));
        out
    }
}
