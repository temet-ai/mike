use super::super::{CheckpointInput, MetaUpdate, WorkThreadError};
use super::{fixture, input, titled};
use crate::store::persistence::launch_repo::LaunchPersist as _;

#[tokio::test]
async fn review_without_acceptance_criteria_is_refused_and_the_status_stays() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    f.handle
        .launch()
        .checkpoint_work_thread(f.project, &titled("gate", "start"))
        .await?;
    let refused = f
        .handle
        .launch()
        .update_work_thread_meta(
            f.project,
            "gate",
            &MetaUpdate {
                status: Some("review"),
                ..MetaUpdate::default()
            },
        )
        .await;
    let Err(WorkThreadError::Invalid(message)) = refused else {
        anyhow::bail!("expected a validation error, got {refused:?}");
    };
    assert!(
        message.contains("`gate`") && message.contains("acceptance criteria"),
        "{message}"
    );
    let blank = f
        .handle
        .launch()
        .update_work_thread_meta(
            f.project,
            "gate",
            &MetaUpdate {
                status: Some("review"),
                acceptance_criteria: Some("  \n "),
                ..MetaUpdate::default()
            },
        )
        .await;
    assert!(matches!(blank, Err(WorkThreadError::Invalid(_))));
    let unchanged = f
        .handle
        .launch()
        .get_work_thread(f.project, "gate", Default::default())
        .await?;
    assert_eq!(unchanged.thread.status.as_deref(), Some("active"));
    assert_eq!(unchanged.acceptance_criteria, None);
    Ok(())
}

#[tokio::test]
async fn review_with_criteria_in_the_same_call_is_accepted() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let outcome = f
        .handle
        .launch()
        .checkpoint_work_thread(
            f.project,
            &CheckpointInput {
                meta: MetaUpdate {
                    title: Some("Gate"),
                    status: Some("review"),
                    acceptance_criteria: Some("A1. the thing works"),
                    ..MetaUpdate::default()
                },
                ..input("gate", "ready")
            },
        )
        .await?;
    assert_eq!(outcome.thread.status.as_deref(), Some("review"));
    let detail = f
        .handle
        .launch()
        .get_work_thread(f.project, "gate", Default::default())
        .await?;
    assert_eq!(
        detail.acceptance_criteria.as_deref(),
        Some("A1. the thing works")
    );
    let cleared = f
        .handle
        .launch()
        .update_work_thread_meta(
            f.project,
            "gate",
            &MetaUpdate {
                acceptance_criteria: Some(""),
                ..MetaUpdate::default()
            },
        )
        .await;
    assert!(matches!(cleared, Err(WorkThreadError::Invalid(_))));
    Ok(())
}

#[tokio::test]
async fn dependencies_replace_by_slug_and_unknown_or_self_slugs_are_refused() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    for slug in ["base", "other", "top"] {
        f.handle
            .launch()
            .checkpoint_work_thread(f.project, &titled(slug, "x"))
            .await?;
    }
    let first = vec!["base".to_owned(), "other".to_owned()];
    f.handle
        .launch()
        .update_work_thread_meta(
            f.project,
            "top",
            &MetaUpdate {
                depends_on: Some(&first),
                ..MetaUpdate::default()
            },
        )
        .await?;
    let second = vec!["other".to_owned()];
    f.handle
        .launch()
        .update_work_thread_meta(
            f.project,
            "top",
            &MetaUpdate {
                depends_on: Some(&second),
                ..MetaUpdate::default()
            },
        )
        .await?;
    let top = f
        .handle
        .launch()
        .get_work_thread(f.project, "top", Default::default())
        .await?;
    assert_eq!(top.depends_on, ["other"]);
    let other = f
        .handle
        .launch()
        .get_work_thread(f.project, "other", Default::default())
        .await?;
    assert_eq!(other.depended_on_by, ["top"]);

    let unknown = vec!["nowhere".to_owned()];
    let refused = f
        .handle
        .launch()
        .update_work_thread_meta(
            f.project,
            "top",
            &MetaUpdate {
                depends_on: Some(&unknown),
                ..MetaUpdate::default()
            },
        )
        .await;
    let Err(WorkThreadError::Invalid(message)) = refused else {
        anyhow::bail!("expected a validation error, got {refused:?}");
    };
    assert!(message.contains("nowhere"), "{message}");
    let itself = vec!["top".to_owned()];
    let refused = f
        .handle
        .launch()
        .update_work_thread_meta(
            f.project,
            "top",
            &MetaUpdate {
                depends_on: Some(&itself),
                ..MetaUpdate::default()
            },
        )
        .await;
    assert!(matches!(refused, Err(WorkThreadError::Invalid(m)) if m.contains("depends_on[0]")));
    Ok(())
}

#[tokio::test]
async fn unknown_status_is_refused_listing_the_allowed_ones_and_empty_strings_clear_fields()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    f.handle
        .launch()
        .checkpoint_work_thread(
            f.project,
            &CheckpointInput {
                meta: MetaUpdate {
                    title: Some("Fields"),
                    branch: Some("feat/x"),
                    worktree: Some(".claude/worktrees/x"),
                    goal: Some("do x"),
                    ..MetaUpdate::default()
                },
                ..input("fields", "x")
            },
        )
        .await?;
    let refused = f
        .handle
        .launch()
        .update_work_thread_meta(
            f.project,
            "fields",
            &MetaUpdate {
                status: Some("finished"),
                ..MetaUpdate::default()
            },
        )
        .await;
    let Err(WorkThreadError::Invalid(message)) = refused else {
        anyhow::bail!("expected a validation error, got {refused:?}");
    };
    assert!(
        message.contains("finished") && message.contains("implementing"),
        "{message}"
    );
    let cleared = f
        .handle
        .launch()
        .update_work_thread_meta(
            f.project,
            "fields",
            &MetaUpdate {
                branch: Some(""),
                worktree: Some(""),
                ..MetaUpdate::default()
            },
        )
        .await?;
    assert_eq!(cleared.branch, None);
    assert_eq!(cleared.worktree, None);
    assert_eq!(cleared.goal.as_deref(), Some("do x"));
    let missing = f
        .handle
        .launch()
        .update_work_thread_meta(f.project, "ghost", &MetaUpdate::default())
        .await;
    assert!(matches!(missing, Err(WorkThreadError::NotFound(_))));
    Ok(())
}
