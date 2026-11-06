use mike::extract::request::{compute_project_root, project_display_name};
use mike::store::persistence::workspace::RegisterWorkspaceSession;

use crate::store::workspace::support::{register_input, shared_handle, shared_pool_and_url};

#[tokio::test]
async fn trimmed_path_resolves_to_the_canonical_project_row() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let Some((pool, _)) = shared_pool_and_url().await? else {
        return Ok(());
    };

    let slug = uuid::Uuid::new_v4();
    let canonical_path = format!("/Users/tester/projects/temet/rc-{slug}");
    let trimmed_path = format!("projects/temet/rc-{slug}");

    let registered = handle
        .register_workspace_session(register_input(uuid::Uuid::new_v4(), &canonical_path))
        .await?;

    let display_name = format!("rc-{slug}");
    sqlx::query!(
        "INSERT INTO projects.projects (project_path, display_name) VALUES ($1, $2)",
        trimmed_path,
        display_name,
    )
    .execute(&pool)
    .await?;

    let by_trimmed = handle.find_project_id_by_path(&trimmed_path).await?;
    assert_eq!(
        by_trimmed,
        Some(registered.project_id),
        "the trimmed form must resolve to the canonical row that holds the registrations"
    );

    let by_absolute = handle.find_project_id_by_path(&canonical_path).await?;
    assert_eq!(by_absolute, Some(registered.project_id));

    let orphan_only = format!("projects/temet/orphan-{}", uuid::Uuid::new_v4());
    sqlx::query!(
        "INSERT INTO projects.projects (project_path, display_name) VALUES ($1, $1)",
        orphan_only,
    )
    .execute(&pool)
    .await?;
    assert!(
        handle
            .find_project_id_by_path(&orphan_only)
            .await?
            .is_some(),
        "a legacy-only project must still resolve"
    );

    assert_eq!(
        handle.find_project_id_by_path("no/such/project").await?,
        None
    );

    pool.close().await;
    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn find_project_id_by_path_exact_match_with_absolute_fallback() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };

    let unique = uuid::Uuid::new_v4();
    let absolute_path = format!("/Users/vimes/projects/temet/temet-proxy-{unique}");
    let canonical = project_display_name(compute_project_root(&absolute_path));

    let session = uuid::Uuid::new_v4();
    let registered = handle
        .register_workspace_session(RegisterWorkspaceSession {
            cwd: &absolute_path,
            ..register_input(session, &canonical)
        })
        .await?;

    let exact = handle.find_project_id_by_path(&canonical).await?;
    assert_eq!(exact, Some(registered.project_id));

    let via_fallback = handle.find_project_id_by_path(&absolute_path).await?;
    assert_eq!(
        via_fallback,
        Some(registered.project_id),
        "an absolute-path caller must resolve to the same unified row"
    );

    let missing = handle
        .find_project_id_by_path(&format!("/no/such/{unique}"))
        .await?;
    assert_eq!(missing, None);

    handle.pool().close().await;
    Ok(())
}
