use crate::store::persistence::launch_repo::LaunchPersist as _;
use uuid::Uuid;

use super::super::WorkThreadError;
use super::fixture;

#[tokio::test]
async fn a_registered_location_outranks_a_bare_project_path_and_two_locations_are_ambiguous()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let duplicate = f
        .handle
        .define_knowledge_project("duplicate", Uuid::new_v4())
        .await?;
    sqlx::query("UPDATE projects.projects SET project_path = '/shared/checkout' WHERE id = $1")
        .bind(duplicate.id)
        .execute(f.handle.pool())
        .await?;
    let canonical = f
        .handle
        .define_knowledge_project("canonical", Uuid::new_v4())
        .await?;
    f.handle
        .add_checkout_location(Uuid::new_v4(), "machine", "/shared/checkout", canonical.id)
        .await?;

    let by_location = f
        .handle
        .launch()
        .resolve_work_thread_project("/shared/checkout")
        .await?;
    assert_eq!(by_location.map(|p| p.id), Some(canonical.id));

    let rival = f
        .handle
        .define_knowledge_project("rival", Uuid::new_v4())
        .await?;
    f.handle
        .add_checkout_location(
            Uuid::new_v4(),
            "other machine",
            "/shared/checkout",
            rival.id,
        )
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
