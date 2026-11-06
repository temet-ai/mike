use super::RequestStore;

impl RequestStore {
    pub(crate) fn is_first_composable_request_in_session(&self, session_id: &str) -> bool {
        !self.composable_session_index.contains_key(session_id)
    }

    pub(crate) fn claim_first_composable_slot(&mut self, session_id: &str) -> bool {
        if self.composable_session_index.contains_key(session_id) {
            return false;
        }
        self.composable_session_index
            .insert(session_id.to_string(), ());
        true
    }

    pub(crate) fn seed_composable_session(&mut self, session_id: &str) {
        self.composable_session_index
            .entry(session_id.to_string())
            .or_default();
    }

    pub(crate) fn established_session_persona(&self, session_id: &str) -> Option<Option<String>> {
        self.established_persona_index.get(session_id).cloned()
    }

    pub(crate) fn establish_session_persona(&mut self, session_id: &str, persona: Option<String>) {
        self.established_persona_index
            .entry(session_id.to_string())
            .or_insert(persona);
    }
}
