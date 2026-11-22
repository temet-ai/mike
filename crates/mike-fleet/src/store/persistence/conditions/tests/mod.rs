mod backoff;
mod crud;
mod holds;
mod next_armed;
mod presence;
mod spec;
mod sql_condition;
mod words;

use chrono::{Duration, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

use super::NewCondition;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::seed_project;

pub use crate::store::persistence::fleet::tests::fixture;

pub fn new_condition(kind: &str, spec: Value) -> NewCondition {
    NewCondition {
        machine_uuid: Uuid::new_v4(),
        created_by_session: Some(Uuid::new_v4()),
        project_id: None,
        thread_slug: None,
        kind: kind.to_owned(),
        spec,
        not_before: Utc::now() - Duration::hours(1),
        deadline: None,
        check_every_secs: 300,
        action: "message".to_owned(),
        target_session: Some(Uuid::new_v4()),
        folder: None,
        brief: "come back to this".to_owned(),
    }
}

pub fn time_condition() -> NewCondition {
    new_condition("time", json!({}))
}

pub struct SeededSession {
    pub uuid: Uuid,
    pub row_id: i64,
    pub project_id: i64,
}

pub async fn seeded_session(handle: &PersistHandle) -> anyhow::Result<SeededSession> {
    let project_id = seed_project(
        handle.pool(),
        &format!("/tmp/conditions-{}", Uuid::new_v4()),
    )
    .await?;
    let uuid = Uuid::new_v4();
    let row_id = crate::store::persistence::fleet::tests::seed::seed_session(
        handle.pool(),
        project_id,
        uuid,
    )
    .await?;
    Ok(SeededSession {
        uuid,
        row_id,
        project_id,
    })
}
