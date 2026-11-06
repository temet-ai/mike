mod evidence;
mod profile;

use uuid::Uuid;

pub(crate) use evidence::{
    AvailableTopicRow, DeliveredLessonRow, DeliveryRecallRow, DeliveryStageRow, DeliveryToolUseRow,
};
pub(crate) use profile::{
    DeliveryAgentRow, DeliveryModelRow, DeliverySessionRow, DeliveryToggleChangeRow,
};

use super::{PersistError, PersistHandle};

pub(crate) struct SessionDeliveryRows {
    pub(crate) session: DeliverySessionRow,
    pub(crate) models: Vec<DeliveryModelRow>,
    pub(crate) agents: Vec<DeliveryAgentRow>,
    pub(crate) toggle_changes: Vec<DeliveryToggleChangeRow>,
    pub(crate) lessons: Vec<DeliveredLessonRow>,
    pub(crate) attributed_lesson_requests: i64,
    pub(crate) available_topics: Vec<AvailableTopicRow>,
    pub(crate) stages: Vec<DeliveryStageRow>,
    pub(crate) recalls: Vec<DeliveryRecallRow>,
    pub(crate) tool_uses: Vec<DeliveryToolUseRow>,
}

impl PersistHandle {
    pub(crate) async fn session_delivery(
        &self,
        session_uuid: Uuid,
        toggle_keys: &[String],
        recall_tools: &[String],
    ) -> Result<Option<SessionDeliveryRows>, PersistError> {
        crate::metrics::time_db("session_delivery", async move {
            let pool = &self.pool;
            let Some(session) = profile::find_session(pool, session_uuid).await? else {
                return Ok(None);
            };
            let session_id = session.id;
            let (models, agents, toggle_changes) = tokio::try_join!(
                profile::models(pool, session_id),
                profile::agents(pool, session_id),
                profile::toggle_changes(pool, session.first_seen, toggle_keys),
            )?;
            let (lessons, attributed_lesson_requests, available_topics) = tokio::try_join!(
                evidence::delivered_lessons(pool, session_id),
                evidence::attributed_lesson_requests(pool, session_id),
                evidence::available_topics(pool, session.project_id),
            )?;
            let (stages, recalls, tool_uses) = tokio::try_join!(
                evidence::stages(pool, session_id),
                evidence::recalls(pool, session_id),
                evidence::tool_uses(pool, session_id, recall_tools),
            )?;
            Ok(Some(SessionDeliveryRows {
                session,
                models,
                agents,
                toggle_changes,
                lessons,
                attributed_lesson_requests,
                available_topics,
                stages,
                recalls,
                tool_uses,
            }))
        })
        .await
    }
}
