use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use super::repo;
use crate::change_record::watch::git::STORE_LIMIT;
use crate::change_record::watch::notifications::{Places, Signal, classify};
use crate::change_record::watch::skip::is_build_output;
use crate::change_record::watch::supervisor::repository_root;
use crate::change_record::watch::tree::{DeltaKind, KnownTree};

#[tokio::test]
async fn a_folder_with_nothing_tracked_does_not_put_its_enclosing_repo_under_watch()
-> anyhow::Result<()> {
    let umbrella = repo().await?;
    umbrella.write("mine/home/photo.png", "pixels")?;
    let mine = umbrella.git.absolute("mine").to_string_lossy().into_owned();
    let src = umbrella.git.absolute("src").to_string_lossy().into_owned();
    assert_eq!(repository_root(&mine).await, None);
    assert_eq!(repository_root(&src).await, Some(umbrella.root.clone()));
    assert_eq!(
        repository_root(&umbrella.root).await,
        Some(umbrella.root.clone())
    );
    Ok(())
}

#[tokio::test]
async fn the_full_check_skips_build_output_that_no_gitignore_names() -> anyhow::Result<()> {
    let repo = repo().await?;
    let mut tree = KnownTree::baseline(&repo.git).await?;
    for path in [
        ".target-dev/debug/libx.rlib",
        "web/dist/app.js",
        "tool/.venv/lib/site.py",
        "py/__pycache__/m.pyc",
        "crate/target/debug/bin",
        "src/new.txt",
    ] {
        repo.write(path, "x")?;
    }
    let deltas = tree.backstop(&repo.git).await?;
    let paths: Vec<&str> = deltas.iter().map(|d| d.path.as_str()).collect();
    assert_eq!(paths, vec!["src/new.txt"]);
    Ok(())
}

#[tokio::test]
async fn an_untracked_file_left_alone_is_not_hashed_again() -> anyhow::Result<()> {
    let repo = repo().await?;
    let old = SystemTime::now() - Duration::from_secs(3600);
    let set_time = |at: SystemTime| -> anyhow::Result<()> {
        std::fs::File::options()
            .write(true)
            .open(repo.git.absolute("notes.txt"))?
            .set_modified(at)?;
        Ok(())
    };
    repo.write("notes.txt", "aaaa")?;
    set_time(old)?;
    let mut tree = KnownTree::baseline(&repo.git).await?;
    repo.write("notes.txt", "bbbb")?;
    set_time(old)?;
    assert!(
        tree.backstop(&repo.git).await?.is_empty(),
        "same size and time: the cached hash stands"
    );
    set_time(old + Duration::from_secs(60))?;
    let deltas = tree.backstop(&repo.git).await?;
    assert_eq!(deltas.len(), 1);
    assert_eq!(deltas[0].change, DeltaKind::Modified);
    Ok(())
}

#[tokio::test]
async fn a_file_over_the_store_limit_is_hashed_but_not_written_to_the_repo() -> anyhow::Result<()> {
    let repo = repo().await?;
    let big = "x".repeat(usize::try_from(STORE_LIMIT)? + 1);
    repo.write("big.bin", &big)?;
    repo.write("small.txt", "small")?;
    let hashed = repo
        .git
        .hash_paths(&["big.bin".to_string(), "small.txt".to_string()])
        .await?;
    assert_eq!(hashed.len(), 2);
    for (path, sha) in hashed {
        let stored = repo.git.run(&["cat-file", "-e", &sha]).await.is_ok();
        assert_eq!(stored, path == "small.txt", "{path} stored={stored}");
    }
    Ok(())
}

#[tokio::test]
async fn a_changed_gitignore_lets_a_path_it_ignored_be_recorded() -> anyhow::Result<()> {
    let repo = repo().await?;
    repo.write(".gitignore", "target/\nnode_modules/\n*.log\n")?;
    repo.git(&["commit", "-qam", "ignore logs"]).await?;
    let mut tree = KnownTree::baseline(&repo.git).await?;
    repo.write("run.log", "one")?;
    let log: BTreeSet<String> = ["run.log".to_string()].into();
    assert!(tree.observe_paths(&repo.git, &log).await?.deltas.is_empty());
    repo.write(".gitignore", "target/\n")?;
    let both: BTreeSet<String> = [".gitignore".to_string(), "run.log".to_string()].into();
    let deltas = tree.observe_paths(&repo.git, &both).await?.deltas;
    assert!(deltas.iter().any(|d| d.path == "run.log"), "{deltas:?}");
    Ok(())
}

#[test]
fn build_output_paths_are_never_signalled() {
    let places = Places {
        root: PathBuf::from("/r"),
        git_dir: PathBuf::from("/r/.git"),
        common_dir: PathBuf::from("/r/.git"),
    };
    assert_eq!(classify(&places, &PathBuf::from("/r/target/debug/x")), None);
    assert_eq!(
        classify(&places, &PathBuf::from("/r/a/node_modules/p/i.js")),
        None
    );
    assert_eq!(
        classify(&places, &PathBuf::from("/r/src/targets.rs")),
        Some(Signal::Path("src/targets.rs".into()))
    );
    assert!(is_build_output(".target-mbx/x"));
    assert!(!is_build_output("src/distance.rs"));
}
