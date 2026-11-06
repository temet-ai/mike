use uuid::Uuid;

use super::super::reads::HistoryPage;
use super::super::{CheckpointInput, StateFields, WorkThreadError};
use super::{fixture, input, titled};

#[tokio::test]
async fn first_checkpoint_creates_the_thread_with_state_history_and_session_link()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let session = Uuid::new_v4();
    let files = vec!["crates/mike/src/main.rs".to_owned()];
    let outcome = f
        .handle
        .checkpoint_work_thread(
            f.project,
            &CheckpointInput {
                session_uuid: Some(session),
                commit_sha: Some("abc1234"),
                contribution: Some("wrote the first checkpoint"),
                state: StateFields {
                    where_we_are: "half way",
                    next_up: "finish it",
                    files: &files,
                    ..StateFields::default()
                },
                ..titled("motion", "Half way through the motion work")
            },
        )
        .await?;
    assert!(outcome.created);
    assert!(!outcome.state_replaced);
    assert_eq!(outcome.entry_ord, 1);
    assert_eq!(outcome.thread.status.as_deref(), Some("active"));
    assert_eq!(outcome.thread.last_session_uuid, Some(session));

    let detail = f
        .handle
        .get_work_thread(f.project, "motion", HistoryPage::default())
        .await?;
    let state = detail
        .state
        .ok_or_else(|| anyhow::anyhow!("no state row"))?;
    assert_eq!(state.tldr, "Half way through the motion work");
    assert_eq!(state.next_up, "finish it");
    assert_eq!(state.files, files);
    assert_eq!(detail.history.len(), 1);
    assert_eq!(detail.history[0].kind, "checkpoint");
    assert_eq!(detail.history[0].commit_sha.as_deref(), Some("abc1234"));
    assert_eq!(detail.sessions.len(), 1);
    assert_eq!(
        detail.sessions[0].contribution,
        "wrote the first checkpoint"
    );
    Ok(())
}

#[tokio::test]
async fn second_checkpoint_replaces_the_state_and_keeps_the_old_one_in_the_history()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    f.handle
        .checkpoint_work_thread(
            f.project,
            &CheckpointInput {
                state: StateFields {
                    where_we_are: "old position",
                    decisions: "use the shared layer",
                    ..StateFields::default()
                },
                ..titled("motion", "old summary")
            },
        )
        .await?;
    let second = f
        .handle
        .checkpoint_work_thread(
            f.project,
            &CheckpointInput {
                state: StateFields {
                    where_we_are: "new position",
                    ..StateFields::default()
                },
                ..input("motion", "new summary")
            },
        )
        .await?;
    assert!(!second.created);
    assert!(second.state_replaced);
    assert_eq!(second.entry_ord, 3);

    let detail = f
        .handle
        .get_work_thread(f.project, "motion", HistoryPage::everything())
        .await?;
    let state = detail
        .state
        .ok_or_else(|| anyhow::anyhow!("no state row"))?;
    assert_eq!(state.tldr, "new summary");
    assert_eq!(state.where_we_are, "new position");
    assert_eq!(state.decisions, "");
    let kinds: Vec<&str> = detail.history.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(kinds, ["checkpoint", "state-replaced", "checkpoint"]);
    let replaced = &detail.history[1];
    let data = replaced
        .data
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("state-replaced entry has no data"))?;
    assert_eq!(data["tldr"], "old summary");
    assert_eq!(data["where_we_are"], "old position");
    assert_eq!(data["decisions"], "use the shared layer");
    Ok(())
}

#[tokio::test]
async fn checkpoint_of_an_unknown_thread_without_a_title_names_the_slug() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let refused = f
        .handle
        .checkpoint_work_thread(f.project, &input("ghost", "nothing"))
        .await;
    let Err(WorkThreadError::Invalid(message)) = refused else {
        anyhow::bail!("expected a validation error, got {refused:?}");
    };
    assert!(
        message.contains("`ghost`") && message.contains("title"),
        "{message}"
    );
    Ok(())
}

#[tokio::test]
async fn tldr_over_600_characters_is_refused_with_its_length() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let long = "x".repeat(612);
    let refused = f
        .handle
        .checkpoint_work_thread(f.project, &titled("long", &long))
        .await;
    let Err(WorkThreadError::Invalid(message)) = refused else {
        anyhow::bail!("expected a validation error, got {refused:?}");
    };
    assert!(
        message.contains("612") && message.contains("12 too many"),
        "{message}"
    );
    assert!(!f.handle.work_thread_exists(f.project, "long").await?);
    Ok(())
}

#[tokio::test]
async fn an_absolute_file_path_is_refused_with_its_position() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let files = vec![
        "ok/path.rs".to_owned(),
        "/Users/me/project/file.rs".to_owned(),
    ];
    let refused = f
        .handle
        .checkpoint_work_thread(
            f.project,
            &CheckpointInput {
                state: StateFields {
                    files: &files,
                    ..StateFields::default()
                },
                ..titled("paths", "tldr")
            },
        )
        .await;
    let Err(WorkThreadError::Invalid(message)) = refused else {
        anyhow::bail!("expected a validation error, got {refused:?}");
    };
    assert!(
        message.contains("files[1]") && message.contains("absolute"),
        "{message}"
    );
    Ok(())
}

#[tokio::test]
async fn a_refused_checkpoint_changes_nothing() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    f.handle
        .checkpoint_work_thread(f.project, &titled("steady", "first"))
        .await?;
    let refused = f
        .handle
        .checkpoint_work_thread(
            f.project,
            &CheckpointInput {
                meta: super::super::MetaUpdate {
                    status: Some("review"),
                    ..Default::default()
                },
                ..input("steady", "second")
            },
        )
        .await;
    assert!(matches!(refused, Err(WorkThreadError::Invalid(_))));
    let detail = f
        .handle
        .get_work_thread(f.project, "steady", HistoryPage::everything())
        .await?;
    assert_eq!(detail.state.map(|s| s.tldr).as_deref(), Some("first"));
    assert_eq!(detail.history.len(), 1);
    assert_eq!(detail.thread.status.as_deref(), Some("active"));
    Ok(())
}
