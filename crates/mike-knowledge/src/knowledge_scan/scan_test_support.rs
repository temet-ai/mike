use crate::store::persistence::PersistHandle;
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::project_knowledge::LocationInput;
use std::fs;
use std::path::Path;
use uuid::Uuid;

pub(super) fn write(path: &Path, text: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}

pub(super) async fn register(
    handle: &PersistHandle,
    project: &str,
    machine: Uuid,
    root: &Path,
) -> anyhow::Result<Uuid> {
    let row = handle
        .knowledge()
        .register_project_location(&LocationInput {
            project: project.into(),
            machine_uuid: machine,
            machine_name: format!("machine-{machine}"),
            root_path: root.to_string_lossy().into_owned(),
        })
        .await?;
    Ok(serde_json::from_value(row["location_uuid"].clone())?)
}

pub(super) async fn live_documents(
    handle: &PersistHandle,
    source: &str,
) -> anyhow::Result<Vec<String>> {
    Ok(sqlx::query_scalar(
        "SELECT d.relative_path FROM memory.knowledge_documents d
           JOIN memory.knowledge_sources s ON s.source_uuid = d.source_uuid
          WHERE s.name = $1 AND d.deleted_at IS NULL ORDER BY d.relative_path COLLATE \"C\"",
    )
    .bind(source)
    .fetch_all(handle.pool())
    .await?)
}
