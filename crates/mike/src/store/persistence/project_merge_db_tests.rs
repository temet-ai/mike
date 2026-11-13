use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::web_repo::WebPersist as _;
use uuid::Uuid;

use super::ProjectMergeError;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::project_knowledge::RepoLocationAdoption;
use crate::store::persistence::test_support::{
    acquire_ephemeral_db, seed_project, seed_two_sessions,
};

async fn project_of(handle: &PersistHandle, session: Uuid) -> anyhow::Result<(i64, String)> {
    Ok(sqlx::query_as(
        "SELECT project_id, project_resolution FROM sessions.sessions WHERE session_uuid = $1",
    )
    .bind(session)
    .fetch_one(handle.pool())
    .await?)
}

#[tokio::test]
async fn worktree_project_folds_into_its_real_project_and_new_captures_follow() -> anyhow::Result<()>
{
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let main = seed_project(handle.pool(), "/src/app").await?;
    let worktree = seed_project(handle.pool(), "/src/app-wt-1").await?;
    let sessions = [Uuid::new_v4(), Uuid::new_v4()];
    seed_two_sessions(
        handle.pool(),
        &sessions[0].to_string(),
        &sessions[1].to_string(),
        worktree,
    )
    .await?;
    let main_uuid: Uuid =
        sqlx::query_scalar("SELECT project_uuid FROM projects.projects WHERE id = $1")
            .bind(main)
            .fetch_one(handle.pool())
            .await?;

    let merged = handle
        .web()
        .merge_project_into("/src/app-wt-1", main_uuid)
        .await
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    assert_eq!(merged.sessions_moved, 2);
    assert_eq!(merged.target_path, "/src/app");
    assert_eq!(
        project_of(&handle, sessions[0]).await?,
        (main, "operator".into())
    );
    assert_eq!(
        handle.upsert_project_returning_id("/src/app-wt-1").await?,
        main
    );
    let listed: Vec<String> = handle
        .launch()
        .list_projects()
        .await?
        .into_iter()
        .map(|row| row.project_path)
        .collect();
    assert!(listed.contains(&"/src/app".to_string()));
    assert!(!listed.contains(&"/src/app-wt-1".to_string()));
    assert!(matches!(
        handle.web().merge_project_into("/src/app", main_uuid).await,
        Err(ProjectMergeError::SameProject)
    ));

    let fresh = Uuid::new_v4();
    let unresolved = seed_project(handle.pool(), "/elsewhere").await?;
    seed_two_sessions(
        handle.pool(),
        &fresh.to_string(),
        &Uuid::new_v4().to_string(),
        unresolved,
    )
    .await?;
    sqlx::query(
        "UPDATE sessions.sessions SET project_resolution = 'unresolved' WHERE session_uuid = $1",
    )
    .bind(fresh)
    .execute(handle.pool())
    .await?;
    assert_eq!(
        handle.knowledge().project_id_for_path("/src/app/").await?,
        Some(main)
    );
    let location = handle
        .knowledge()
        .adopt_repo_location(&RepoLocationAdoption {
            session: fresh,
            machine: Uuid::new_v4(),
            machine_name: "host",
            checkout_root: "/src/wt/P1",
            project_id: main,
        })
        .await?
        .ok_or_else(|| anyhow::anyhow!("unbound session must adopt the worktree location"))?;
    handle
        .knowledge()
        .bind_session_location(fresh, location)
        .await?;
    assert_eq!(project_of(&handle, fresh).await?.0, main);

    db.cleanup().await
}
