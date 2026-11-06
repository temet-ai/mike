use super::RequestStore;
use super::models::RequestId;

impl RequestStore {
    pub(super) fn detach_from_session_index(&mut self, session_id: &str, id: RequestId) {
        if let Some(bucket) = self.session_index.get_mut(session_id) {
            bucket.retain(|bid| *bid != id);
            if bucket.is_empty() {
                self.session_index.remove(session_id);
                self.session_meta.remove(session_id);
            }
        }
    }

    pub(crate) fn requests_by_session_id(&self, session_id: &str) -> Vec<RequestId> {
        let mut ids: Vec<RequestId> = self
            .session_index
            .get(session_id)
            .map(|bucket| bucket.iter().copied().collect())
            .unwrap_or_default();
        ids.sort_by_key(|id| id.as_u64());
        ids
    }
}
