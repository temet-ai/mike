use std::sync::Arc;

use crate::api::poll_answers::PollAnswers;
use crate::base_state::BaseState;

pub trait SessionProjects: Send + Sync {
    fn note(&self, session_id: String, project_path: String);
}

#[derive(Clone)]
pub struct LaunchState {
    pub base: BaseState,
    pub poll_answers: PollAnswers,
    pub session_projects: Arc<dyn SessionProjects>,
}

impl std::ops::Deref for LaunchState {
    type Target = BaseState;

    fn deref(&self) -> &BaseState {
        &self.base
    }
}
