use super::*;
use chrono::TimeZone;

fn at(minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 20, 19, minute, 0)
        .single()
        .unwrap_or_default()
}

#[test]
fn a_later_report_naming_a_commit_moves_the_task_to_that_commit() {
    let commits = vec![(json!({"task": "T2", "sha": "own"}), at(21))];
    let reports = vec![
        (json!({"task": "T2", "commit": "later"}), at(40)),
        (json!({"task": "T9", "commit": "theirs"}), at(50)),
    ];
    assert_eq!(
        judged_commit_of(&commits, &reports, "T2").as_deref(),
        Some("later")
    );
    let early = vec![(json!({"task": "T2", "commit": "early"}), at(10))];
    assert_eq!(
        judged_commit_of(&commits, &early, "T2").as_deref(),
        Some("own")
    );
}

fn run(repo: &std::path::Path, args: &[&str]) -> anyhow::Result<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()?;
    anyhow::ensure!(out.status.success(), "git {args:?} failed");
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[tokio::test]
async fn a_named_commit_must_contain_the_tasks_commit_and_be_on_the_branch() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let repo = dir.path();
    run(repo, &["init", "-q", "-b", "main"])?;
    run(repo, &["commit", "-q", "--allow-empty", "-m", "own"])?;
    let own = run(repo, &["rev-parse", "HEAD"])?;
    run(repo, &["commit", "-q", "--allow-empty", "-m", "fix"])?;
    let later = run(repo, &["rev-parse", "HEAD"])?;
    run(repo, &["checkout", "-q", "-b", "side", &own])?;
    run(repo, &["commit", "-q", "--allow-empty", "-m", "side"])?;
    let side = run(repo, &["rev-parse", "HEAD"])?;
    run(repo, &["checkout", "-q", "main"])?;
    let path = repo.to_string_lossy();

    assert_eq!(
        named_commit(&path, "main", Some(&own), &later).await,
        Ok(later.clone())
    );
    assert!(
        named_commit(&path, "main", Some(&later), &own)
            .await
            .is_err()
    );
    assert!(
        named_commit(&path, "main", Some(&own), &side)
            .await
            .is_err()
    );
    assert!(
        named_commit(&path, "main", Some(&own), "nope")
            .await
            .is_err()
    );
    Ok(())
}

#[test]
fn a_commit_outside_every_check_path_is_reported_as_not_covered() {
    let covered = vec!["crates".to_string()];
    let docs = vec!["docs/guide.md".to_string()];
    let code = vec![
        "docs/guide.md".to_string(),
        "crates/a/src/lib.rs".to_string(),
    ];
    assert!(outside_check(&docs, &covered));
    assert!(!outside_check(&code, &covered));
    assert!(!outside_check(&docs, &[]));
    assert!(!outside_check(&[], &covered));
}

#[tokio::test]
async fn changed_files_lists_the_files_one_commit_touched() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let repo = dir.path();
    run(repo, &["init", "-q", "-b", "main"])?;
    std::fs::create_dir_all(repo.join("docs"))?;
    std::fs::write(repo.join("docs/guide.md"), "one")?;
    run(repo, &["add", "docs/guide.md"])?;
    run(repo, &["commit", "-q", "-m", "docs"])?;
    let sha = run(repo, &["rev-parse", "HEAD"])?;
    let files = changed_files(&repo.to_string_lossy(), &sha).await;
    assert_eq!(files, vec!["docs/guide.md".to_string()]);
    Ok(())
}
