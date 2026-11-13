use std::sync::Arc;

use crate::launch::state::{LaunchState, SessionProjects};
use crate::lesson_cache::LessonCache;

use super::AppState;

pub struct LessonSessionProjects(pub LessonCache);

impl SessionProjects for LessonSessionProjects {
    fn note(&self, session_id: String, project_path: String) {
        self.0.rcu(|current| {
            Arc::new(current.with_session_project(session_id.clone(), project_path.clone()))
        });
    }
}

impl AppState {
    pub fn launch_state(&self) -> LaunchState {
        LaunchState {
            base: self.base.clone(),
            poll_answers: self.poll_answers.clone(),
            session_projects: Arc::new(LessonSessionProjects(self.lesson_cache.clone())),
        }
    }
}
