use super::llm::Failure;
use super::settings::Settings;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use chrono::{DateTime, Utc};

pub const NEAREST_K: i64 = 10;

pub struct NewlyStated {
    pub statement_id: i64,
    pub embedding: String,
}

pub struct GroupOutcome {
    pub joined: u32,
    pub created: u32,
}

pub async fn grouping_similarity(
    persist: &PersistHandle,
    project_id: Option<i64>,
    settings: &Settings,
) -> Result<f32, Failure> {
    let stored = persist
        .knowledge()
        .latest_grouping_distance(project_id)
        .await
        .map_err(|e| Failure::transient(e.to_string()))?;
    Ok(stored.unwrap_or(settings.grouping_similarity))
}

pub async fn group_statements(
    persist: &PersistHandle,
    settings: &Settings,
    project_id: Option<i64>,
    statements: &[NewlyStated],
    excluded_subject: Option<i64>,
) -> Result<GroupOutcome, Failure> {
    let threshold = f64::from(grouping_similarity(persist, project_id, settings).await?);
    let judge_after: DateTime<Utc> = Utc::now() + settings.settle;
    let db = |e: crate::store::persistence::PersistError| Failure::transient(e.to_string());
    let mut outcome = GroupOutcome {
        joined: 0,
        created: 0,
    };
    for statement in statements {
        let near = persist
            .knowledge()
            .nearest_statements(project_id, &statement.embedding, &[statement.statement_id], NEAREST_K)
            .await
            .map_err(db)?;
        let nearest = near
            .into_iter()
            .find(|n| excluded_subject.is_none() || n.subject_id != excluded_subject);
        let target = nearest
            .filter(|n| n.similarity >= threshold)
            .and_then(|n| n.subject_id);
        match target {
            Some(subject_id) => {
                persist
                    .knowledge()
                    .add_statement_to_subject(statement.statement_id, subject_id, judge_after)
                    .await
                    .map_err(db)?;
                outcome.joined += 1;
            }
            None => {
                persist
                    .knowledge()
                    .create_knowledge_subject(project_id, statement.statement_id, judge_after)
                    .await
                    .map_err(db)?;
                outcome.created += 1;
            }
        }
    }
    Ok(outcome)
}
