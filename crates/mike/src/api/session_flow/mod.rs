pub(crate) mod dto;
mod export;
mod export_trace;
mod handler;
mod map;

pub(crate) use export::get_session_flow_export;
pub(crate) use handler::get_session_flow;

#[cfg(test)]
mod export_merge_tests;
#[cfg(test)]
mod export_tests;

impl dto::SessionFlowCursor {
    pub(crate) fn matches_scope(
        &self,
        session_id: &str,
        component: &dto::SessionFlowComponent,
    ) -> bool {
        let bounds = self
            .after_id
            .parse::<i64>()
            .ok()
            .zip(self.upper_id.parse::<i64>().ok());
        self.component != dto::SessionFlowComponent::Embeddings
            && self.version == 1
            && self.session_id == session_id
            && &self.component == component
            && self.filter == "all"
            && self.after_id.bytes().all(|byte| byte.is_ascii_digit())
            && self.upper_id.bytes().all(|byte| byte.is_ascii_digit())
            && bounds.is_some_and(|(after, upper)| after > 0 && after <= upper)
    }
}
