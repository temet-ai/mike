use super::{plain_dir, repo};
use crate::change_record::writers::{WriterError, paths_named};

#[tokio::test]
async fn paths_named_commit_id_gives_that_commits_files() -> anyhow::Result<()> {
    let repo = repo()?;
    let text = format!("landed in {}", repo.first_commit);
    let found = paths_named(&text, repo.path()).await?;
    assert!(found.contains(&"src/lib.rs".to_string()), "{found:?}");
    assert!(found.contains(&"README.md".to_string()), "{found:?}");
    Ok(())
}

#[tokio::test]
async fn paths_named_tracked_path_is_returned_and_untracked_is_not() -> anyhow::Result<()> {
    let repo = repo()?;
    let found = paths_named("fix src/lib.rs:4 and also src/missing.rs", repo.path()).await?;
    assert_eq!(found, vec!["src/lib.rs".to_string()]);
    Ok(())
}

#[tokio::test]
async fn paths_named_absolute_path_inside_the_checkout_is_made_relative() -> anyhow::Result<()> {
    let repo = repo()?;
    let text = format!("see {}/src/lib.rs", repo.checkout());
    assert_eq!(
        paths_named(&text, repo.path()).await?,
        vec!["src/lib.rs".to_string()]
    );
    Ok(())
}

#[tokio::test]
async fn paths_named_unknown_commit_like_word_gives_nothing() -> anyhow::Result<()> {
    let repo = repo()?;
    assert!(
        paths_named("abcdef1234567890", repo.path())
            .await?
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn paths_named_folder_that_is_not_a_repository_errs() -> anyhow::Result<()> {
    let dir = plain_dir()?;
    let result = paths_named("src/lib.rs", dir.path()).await;
    assert!(
        matches!(result, Err(WriterError::NotARepository(_))),
        "{result:?}"
    );
    Ok(())
}

#[tokio::test]
async fn paths_named_missing_folder_errs() -> anyhow::Result<()> {
    let dir = plain_dir()?;
    let gone = dir.path().join("gone");
    let result = paths_named("src/lib.rs", &gone).await;
    assert!(
        matches!(result, Err(WriterError::NotARepository(_))),
        "{result:?}"
    );
    Ok(())
}
