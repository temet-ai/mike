use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use uuid::Uuid;

use super::super::WorkThreadError;
use super::fixture;
use crate::store::persistence::project_knowledge::LocationInput;

#[tokio::test]
async fn a_registered_location_outranks_a_bare_project_path_and_two_locations_are_ambiguous()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let duplicate = f
        .handle
        .knowledge()
        .define_knowledge_project("duplicate", Uuid::new_v4())
        .await?;
    sqlx::query("UPDATE projects.projects SET project_path = '/shared/checkout' WHERE id = $1")
        .bind(duplicate.id)
        .execute(f.handle.pool())
        .await?;
    let canonical = f
        .handle
        .knowledge()
        .define_knowledge_project("canonical", Uuid::new_v4())
        .await?;
    f.handle
        .knowledge()
        .register_project_location(&LocationInput {
            project: canonical.project_uuid.to_string(),
            machine_uuid: Uuid::new_v4(),
            machine_name: "machine".to_owned(),
            root_path: "/shared/checkout".to_owned(),
        })
        .await?;

    let by_location = f
        .handle
        .launch()
        .resolve_work_thread_project("/shared/checkout")
        .await?;
    assert_eq!(by_location.map(|p| p.id), Some(canonical.id));

    let rival = f
        .handle
        .knowledge()
        .define_knowledge_project("rival", Uuid::new_v4())
        .await?;
    f.handle
        .knowledge()
        .register_project_location(&LocationInput {
            project: rival.project_uuid.to_string(),
            machine_uuid: Uuid::new_v4(),
            machine_name: "other machine".to_owned(),
            root_path: "/shared/checkout".to_owned(),
        })
        .await?;
    let ambiguous = f
        .handle
        .launch()
        .resolve_work_thread_project("/shared/checkout")
        .await;
    assert!(
        matches!(&ambiguous, Err(WorkThreadError::Invalid(m)) if m.contains("project UUID")),
        "{ambiguous:?}"
    );
    sqlx::query("UPDATE projects.projects SET merged_into_id = $2 WHERE id = $1")
        .bind(rival.id)
        .bind(canonical.id)
        .execute(f.handle.pool())
        .await?;

    sqlx::query("UPDATE projects.projects SET merged_into_id = $2 WHERE id = $1")
        .bind(duplicate.id)
        .bind(canonical.id)
        .execute(f.handle.pool())
        .await?;
    let resolved = f
        .handle
        .launch()
        .resolve_work_thread_project("/shared/checkout")
        .await?;
    assert_eq!(resolved.map(|p| p.id), Some(canonical.id));
    let by_uuid = f
        .handle
        .launch()
        .resolve_work_thread_project(&duplicate.project_uuid.to_string())
        .await?;
    assert_eq!(by_uuid.map(|p| p.id), Some(canonical.id));
    assert!(
        f.handle
            .launch()
            .resolve_work_thread_project("/nowhere")
            .await?
            .is_none()
    );
    Ok(())
}
