use uuid::Uuid;

use crate::store::persistence::session_parent::db_tests::Fixture;

#[tokio::test]
async fn ensure_machine_home_location_names_the_project_after_the_machine() -> anyhow::Result<()> {
    let fx = Fixture::new().await?;

    let first = fx.handle.ensure_machine_home_location(fx.machine, "/home/u/").await?;
    let again = fx.handle.ensure_machine_home_location(fx.machine, "/home/u").await?;

    assert!(first.is_some());
    assert_eq!(first, again);
    let (path, name): (String, String) = sqlx::query_as(
        "SELECT p.project_path, p.display_name FROM projects.locations l
           JOIN projects.projects p ON p.id = l.project_id WHERE l.location_uuid = $1",
    )
    .bind(first)
    .fetch_one(fx.handle.pool())
    .await?;
    assert_eq!(path, format!("machine:{}", fx.machine));
    assert_eq!(name, "host home");
    Ok(())
}

#[tokio::test]
async fn ensure_machine_home_location_unknown_machine_is_none() -> anyhow::Result<()> {
    let fx = Fixture::new().await?;
    let placed = fx
        .handle
        .ensure_machine_home_location(Uuid::new_v4(), "/home/u")
        .await?;
    assert!(placed.is_none());
    Ok(())
}

#[tokio::test]
async fn unresolved_home_roots_lists_home_cwds_of_unresolved_sessions() -> anyhow::Result<()> {
    let fx = Fixture::new().await?;
    let home = format!("/home/fixture-{}", Uuid::new_v4().simple());
    fx.unresolved(Some(&home)).await?;
    fx.unresolved(Some(&format!("{home}/.config/x"))).await?;
    fx.unresolved(Some("/tmp/elsewhere")).await?;

    let roots = fx.handle.unresolved_home_roots().await?;

    let found = roots
        .iter()
        .find(|root| root.home_root == home)
        .ok_or_else(|| anyhow::anyhow!("home root missing"))?;
    assert_eq!(found.sessions, 2);
    assert_eq!(found.machine_uuid, fx.machine);
    Ok(())
}

#[tokio::test]
async fn backfill_home_subfolder_takes_the_machine_home_location() -> anyhow::Result<()> {
    let fx = Fixture::new().await?;
    let home = format!("/home/fixture-{}", Uuid::new_v4().simple());
    let repo = fx.project(&format!("{home}/projects/repo")).await?;
    fx.location(repo, &format!("{home}/projects/repo")).await?;
    let location = fx
        .handle
        .ensure_machine_home_location(fx.machine, &home)
        .await?;
    let session = fx.unresolved(Some(&format!("{home}/.config/hypr"))).await?;
    let in_repo = fx.unresolved(Some(&format!("{home}/projects/repo/src"))).await?;

    let proposals = fx.handle.propose_unresolved_session_projects(None).await?;

    let home_pick = proposals.iter().find(|p| p.session_uuid == session);
    assert_eq!(home_pick.and_then(|p| p.location_uuid), location);
    let repo_pick = proposals.iter().find(|p| p.session_uuid == in_repo);
    assert_eq!(repo_pick.map(|p| p.project_id), Some(repo));
    Ok(())
}

#[tokio::test]
async fn live_local_root_never_prefix_matches_the_machine_home() -> anyhow::Result<()> {
    let fx = Fixture::new().await?;
    let home = format!("/home/fixture-{}", Uuid::new_v4().simple());
    fx.handle
        .ensure_machine_home_location(fx.machine, &home)
        .await?;
    let cwd = format!("{home}/Documents/cad");
    let session = fx.unresolved(Some(&cwd)).await?;

    let bound = fx
        .handle
        .bind_session_to_local_root(session, fx.machine, &cwd)
        .await?;

    assert!(bound.is_none());
    Ok(())
}

#[tokio::test]
async fn backfill_folder_of_repos_under_home_is_left_out() -> anyhow::Result<()> {
    let fx = Fixture::new().await?;
    let home = format!("/home/fixture-{}", Uuid::new_v4().simple());
    let repo = fx.project(&format!("{home}/projects/trainer/core")).await?;
    fx.location(repo, &format!("{home}/projects/trainer/core")).await?;
    fx.handle
        .ensure_machine_home_location(fx.machine, &home)
        .await?;
    let session = fx.unresolved(Some(&format!("{home}/projects/trainer"))).await?;

    let proposals = fx.handle.propose_unresolved_session_projects(None).await?;

    assert!(proposals.iter().all(|p| p.session_uuid != session));
    assert!(
        fx.handle
            .folder_holds_other_projects(fx.machine, &format!("{home}/projects"))
            .await?
    );
    Ok(())
}
