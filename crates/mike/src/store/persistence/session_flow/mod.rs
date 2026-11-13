pub mod build;
mod cursor;
mod dispatches;
mod enums;
mod label;
mod messages;
mod mike;
mod mike_evaluations;
mod mike_interventions;
mod mike_llm;
mod mike_memory;
mod mike_requests;
mod requests;
mod rows;
mod status;
mod tools;

use super::PersistError;
use chrono::{DateTime, Utc};
pub use enums::*;
pub use rows::*;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum FlowReadError {
    #[error("invalid Flow page or cursor scope")]
    InvalidCursor,
    #[error("Flow component is not implemented")]
    UnsupportedComponent,
    #[error(transparent)]
    Database(#[from] PersistError),
}

impl From<sqlx::Error> for FlowReadError {
    fn from(value: sqlx::Error) -> Self {
        Self::Database(value.into())
    }
}

struct Admission {
    id: i64,
    flow_capture_version: Option<i32>,
    flow_admitted_at: Option<DateTime<Utc>>,
    flow_producer: Option<String>,
}

impl crate::store::persistence::web_repo::WebDb<'_> {
    pub async fn fetch_session_flow(
        self,
        session: Uuid,
        component: FlowComponent,
        limit: u16,
        cursor: Option<&FlowCursor>,
    ) -> Result<Option<FlowPage>, FlowReadError> {
        cursor::validate(session, &component, limit, cursor)?;
        let driver = if component == FlowComponent::Embeddings {
            None
        } else {
            Some(cursor::Driver::for_component(&component)?)
        };
        let mut tx = self.pool.begin().await?;
        sqlx::query!("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
        let admission = sqlx::query_as!(
            Admission,
            "SELECT id, flow_capture_version, flow_admitted_at, flow_producer
             FROM sessions.sessions WHERE session_uuid = $1",
            session,
        )
        .fetch_optional(&mut *tx)
        .await?;
        let Some(admission) = admission else {
            tx.commit().await?;
            return Ok(None);
        };
        let mut page = build::empty_page(session, component.clone(), availability(&admission));
        if matches!(page.availability, FlowAvailability::Eligible { .. }) {
            if let Some(driver) = driver {
                let window = driver
                    .window(&mut tx, admission.id, session, limit, cursor)
                    .await?;
                read_component(&mut tx, &mut page, &window.ids).await?;
                window.apply(&mut page);
                if page.coverage.retention == FlowRetention::Unknown {
                    page.coverage.retention = FlowRetention::Retained;
                }
            } else {
                mike_memory::embedding(&mut tx, &mut page).await?;
            }
            build::finish(&mut page);
        }
        tx.commit().await?;
        Ok(Some(page))
    }
}

fn availability(row: &Admission) -> FlowAvailability {
    match (
        row.flow_capture_version,
        row.flow_admitted_at,
        row.flow_producer.as_ref(),
    ) {
        (Some(1), Some(admitted_at), Some(producer)) => FlowAvailability::Eligible {
            capture_version: 1,
            admitted_at,
            producer: producer.clone(),
        },
        (Some(version), _, _) => match u16::try_from(version) {
            Ok(capture_version) => FlowAvailability::UnsupportedVersion { capture_version },
            Err(_) => FlowAvailability::NoAdmittedStart,
        },
        _ => FlowAvailability::NoAdmittedStart,
    }
}

pub async fn read_component(
    tx: &mut sqlx::PgConnection,
    page: &mut FlowPage,
    ids: &[i64],
) -> Result<(), FlowReadError> {
    match page.coverage.component {
        FlowComponent::Requests => requests::read(tx, page, ids).await?,
        FlowComponent::Dispatches => dispatches::read(tx, page, ids).await?,
        FlowComponent::Tools => tools::read_uses(tx, page, ids).await?,
        FlowComponent::Results => tools::read_results(tx, page, ids).await?,
        FlowComponent::Messages => messages::read(tx, page, ids).await?,
        FlowComponent::WorkerStatus => status::read(tx, page, ids).await?,
        FlowComponent::ManifestStages => mike_requests::manifests(tx, page, ids).await?,
        FlowComponent::LessonInjections => mike_requests::injections(tx, page, ids).await?,
        FlowComponent::VerifierEvaluations => mike_evaluations::evaluations(tx, page, ids).await?,
        FlowComponent::Judgments => mike_evaluations::judgments(tx, page, ids).await?,
        FlowComponent::LlmCalls => mike_llm::read(tx, page, ids).await?,
        FlowComponent::Consolidation => mike_memory::consolidation(tx, page, ids).await?,
        FlowComponent::Interventions => mike_interventions::read(tx, page, ids).await?,
        FlowComponent::Embeddings => return Err(FlowReadError::UnsupportedComponent),
    }
    Ok(())
}
