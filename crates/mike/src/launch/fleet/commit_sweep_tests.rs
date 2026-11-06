use serde_json::json;
use uuid::Uuid;

use super::super::commit::{CommitRequest, commit};
use super::super::definition::FleetDefinition;
use super::super::seated::Seated;
use super::super::tests::{complete, parsed};
use super::super::watch::git;
use super::{NO_DB, scratch_repo};

fn nested_def(repo: &str) -> FleetDefinition {
    let mut value = complete();
    value["repo"] = json!(repo);
    value["tasks"] = json!({
        "M": {"name": "mine", "discipline": "db-dev", "criteria": ["C1"], "files": ["mine"],
              "brief": "x"},
        "T": {"name": "nested", "discipline": "db-dev", "criteria": ["C2"], "files": ["mine/t"],
              "brief": "x"}
    });
    value["shared_files"] = json!([]);
    parsed(&value)
}

fn both_seats(session: Uuid, peer: Uuid) -> Seated {
    Seated::default()
        .worker("M", session, "db-dev-M")
        .worker("T", peer, "db-dev-T")
}

async fn git_text(repo: &str, args: &[&str]) -> anyhow::Result<String> {
    let out = git(repo, args, None)
        .await
        .ok_or_else(|| anyhow::anyhow!("git {args:?} failed"))?;
    Ok(String::from_utf8(out)?)
}

async fn two_tasks_changed_under_one_folder(dir: &std::path::Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir.join("mine/t"))?;
    std::fs::write(dir.join("mine/a.txt"), "two\n")?;
    std::fs::write(dir.join("mine/t/c.txt"), "peer\n")?;
    Ok(())
}

#[tokio::test]
async fn a_named_folder_commit_leaves_a_nested_peer_task_out_and_names_it() -> anyhow::Result<()> {
    let (dir, repo) = scratch_repo().await?;
    two_tasks_changed_under_one_folder(dir.path()).await?;
    let session = Uuid::from_u128(21);
    let def = nested_def(&repo);
    let report = commit(
        &def,
        &both_seats(session, Uuid::from_u128(22)),
        &CommitRequest {
            session,
            message: "feat: mine".into(),
            paths: vec!["mine".into()],
            allow_empty: false,
        },
        NO_DB,
    )
    .await;
    assert_eq!(report.refused, None, "{report:?}");
    assert_eq!(report.paths, vec!["mine/a.txt".to_string()]);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("mine/t/c.txt") && w.contains("another task")),
        "{:?}",
        report.warnings
    );
    let shown = git_text(&repo, &["show", "--name-only", "--format=", "HEAD"]).await?;
    assert!(shown.contains("mine/a.txt"), "{shown}");
    assert!(!shown.contains("mine/t/c.txt"), "{shown}");
    let status = git_text(&repo, &["status", "--porcelain", "--untracked-files=all"]).await?;
    assert!(status.contains("mine/t/c.txt"), "{status}");
    Ok(())
}

#[tokio::test]
async fn an_unnamed_commit_takes_its_own_files_under_a_folder_and_leaves_the_nested_task_out()
-> anyhow::Result<()> {
    let (dir, repo) = scratch_repo().await?;
    two_tasks_changed_under_one_folder(dir.path()).await?;
    let session = Uuid::from_u128(23);
    let def = nested_def(&repo);
    let report = commit(
        &def,
        &both_seats(session, Uuid::from_u128(24)),
        &CommitRequest {
            session,
            message: "feat: mine".into(),
            paths: vec![],
            allow_empty: false,
        },
        NO_DB,
    )
    .await;
    assert_eq!(report.refused, None, "{report:?}");
    assert_eq!(report.paths, vec!["mine/a.txt".to_string()]);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("mine/t/c.txt") && w.contains("left uncommitted")),
        "{:?}",
        report.warnings
    );
    let shown = git_text(&repo, &["show", "--name-only", "--format=", "HEAD"]).await?;
    assert!(!shown.contains("mine/t/c.txt"), "{shown}");
    Ok(())
}
