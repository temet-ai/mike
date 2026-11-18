use serde_json::{Value, json};

use crate::store::persistence::PersistError;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::project_knowledge::KnowledgeQuery;

fn is_empty_first_page(answer: &Value, input: &KnowledgeQuery) -> bool {
    input.offset.unwrap_or(0) == 0
        && answer["items"]
            .as_array()
            .is_some_and(|items| items.is_empty())
}

pub async fn attach_twins(
    handle: &PersistHandle,
    project_id: i64,
    input: &KnowledgeQuery,
    answer: &mut Value,
) -> Result<(), PersistError> {
    if !is_empty_first_page(answer, input) {
        return Ok(());
    }
    let twins = handle.knowledge().knowledge_twins(project_id).await?;
    if !twins.is_empty() {
        answer["twins"] = json!(twins);
    }
    Ok(())
}

#[cfg(test)]
#[path = "twins_tests.rs"]
mod twins_tests;
