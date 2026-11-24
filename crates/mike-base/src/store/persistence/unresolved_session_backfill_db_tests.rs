use uuid::Uuid;

use crate::store::persistence::session_parent::db_tests::Fixture;

fn proposal_for(
    proposals: &[super::ProposedSessionProject],
    session: Uuid,
) -> Option<&super::ProposedSessionProject> {
    proposals.iter().find(|p| p.session_uuid == session)
}

#[tokio::test]
async fn propose_scratchpad_child_gets_parent_project() -> anyhow::Result<()> {
    let fx = Fixture::new().await?;
    let project = fx.project("/repo/ask-kit").await?;
    let location = fx.location(project, "/repo/ask-kit").await?;
    let parent = fx.resolved(project, "operator", Some(location)).await?;
    let child = fx
        .unresolved(Some(&format!("/tmp/claude-1000/-repo-ask-kit/{parent}/scratchpad/bench")))
        .await?;

    let proposals = fx.handle.propose_unresolved_session_projects(None).await?;

    let found = proposal_for(&proposals, child).ok_or_else(|| anyhow::anyhow!("no proposal"))?;
    assert_eq!(found.rule, "parent_scratchpad");
    assert_eq!(found.project_id, project);
    assert_eq!(found.location_uuid, Some(location));
    Ok(())
}

#[tokio::test]
async fn propose_cwd_inside_location_gets_deepest_location() -> anyhow::Result<()> {
    let fx = Fixture::new().await?;
    let outer = fx.project("/data/mono").await?;
    fx.location(outer, "/data/mono").await?;
    let session = fx.unresolved(Some("/data/mono/src")).await?;

    let proposals = fx.handle.propose_unresolved_session_projects(None).await?;

    let found = proposal_for(&proposals, session).ok_or_else(|| anyhow::anyhow!("no proposal"))?;
    assert_eq!(found.rule, "location");
    assert_eq!(found.project_id, outer);
    Ok(())
}

#[tokio::test]
async fn propose_cwd_under_folder_of_projects_is_left_out() -> anyhow::Result<()> {
    let fx = Fixture::new().await?;
    let folder = fx.project("/data/projects").await?;
    fx.location(folder, "/data/projects").await?;
    let repo = fx.project("/data/projects/repo").await?;
    fx.location(repo, "/data/projects/repo").await?;
    let session = fx.unresolved(Some("/data/projects/unknown")).await?;
    let home = fx.unresolved(Some("/home/someone")).await?;

    let proposals = fx.handle.propose_unresolved_session_projects(None).await?;

    assert!(proposal_for(&proposals, session).is_none());
    assert!(proposal_for(&proposals, home).is_none());
    Ok(())
}

#[tokio::test]
async fn propose_wright_engineer_gets_mike_main_checkout() -> anyhow::Result<()> {
    let fx = Fixture::new().await?;
    let mike = fx.project("/repo/mike").await?;
    let main = fx.location(mike, "/repo/mike").await?;
    let session = fx.unresolved(None).await?;
    sqlx::query(
        "INSERT INTO wright.investigations (mode, session_uuid, state, host, machine_uuid, deadline_at, ended_at)
         VALUES ('investigate', $1, 'finished', 'host', $2, now(), now())",
    )
    .bind(session)
    .bind(fx.machine)
    .execute(fx.handle.pool())
    .await?;

    let proposals = fx.handle.propose_unresolved_session_projects(Some(mike)).await?;

    let found = proposal_for(&proposals, session).ok_or_else(|| anyhow::anyhow!("no proposal"))?;
    assert_eq!(found.rule, "wright_engineer");
    assert_eq!(found.location_uuid, Some(main));
    Ok(())
}

#[tokio::test]
async fn apply_proposal_binds_location_and_is_idempotent() -> anyhow::Result<()> {
    let fx = Fixture::new().await?;
    let project = fx.project("/data/repo").await?;
    let location = fx.location(project, "/data/repo").await?;
    let session = fx.unresolved(Some("/data/repo")).await?;
    let proposals = fx.handle.propose_unresolved_session_projects(None).await?;
    let found = proposal_for(&proposals, session)
        .ok_or_else(|| anyhow::anyhow!("no proposal"))?
        .clone();

    fx.handle.apply_proposed_session_project(&found).await?;
    fx.handle.apply_proposed_session_project(&found).await?;

    let bound = fx.bound(session).await?;
    assert_eq!(bound.location, Some(location));
    assert_eq!(bound.project_id, project);
    let again = fx.handle.propose_unresolved_session_projects(None).await?;
    assert!(proposal_for(&again, session).is_none());
    Ok(())
}
