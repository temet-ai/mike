use super::{Repo, db, quick, repo, stop, wait_for, watching};
use crate::change_record::{ChangeEvent, ChangeKind};

async fn with_remote(repo: &Repo) -> anyhow::Result<tempfile::TempDir> {
    let remote = tempfile::tempdir()?;
    let url = remote.path().to_string_lossy().into_owned();
    repo.git(&["init", "-q", "--bare", &url]).await?;
    repo.git(&["remote", "add", "origin", &url]).await?;
    repo.git(&["push", "-q", "origin", "main"]).await?;
    Ok(remote)
}

fn of_kind(events: &[ChangeEvent], kind: ChangeKind) -> Vec<&ChangeEvent> {
    events.iter().filter(|e| e.kind == kind).collect()
}

#[tokio::test]
async fn a_commit_and_a_push_give_one_event_each_with_the_commits_paths() -> anyhow::Result<()> {
    let repo = repo().await?;
    let _remote = with_remote(&repo).await?;
    let Some(db) = db(&repo.root).await? else {
        return Ok(());
    };
    let running = watching(&db, &repo, quick(true)).await?;
    repo.write("src/a.txt", "committed")?;
    repo.git(&["add", "src/a.txt"]).await?;
    repo.git(&["commit", "-q", "-m", "change a"]).await?;
    let sha = repo.git(&["rev-parse", "HEAD"]).await?;
    repo.git(&["push", "-q", "origin", "main"]).await?;
    let seen = wait_for(&db, &repo.root, "the commit and the push", |e| {
        !of_kind(e, ChangeKind::Commit).is_empty() && !of_kind(e, ChangeKind::Push).is_empty()
    })
    .await?;
    let commits = of_kind(&seen, ChangeKind::Commit);
    assert_eq!(commits.len(), 1, "one commit event: {seen:?}");
    assert_eq!(commits[0].paths, vec!["src/a.txt".to_string()]);
    assert_eq!(commits[0].subject, "change a");
    assert_eq!(commits[0].detail["sha"], sha.as_str());
    assert_eq!(commits[0].detail["branch"], "main");
    let pushes = of_kind(&seen, ChangeKind::Push);
    assert_eq!(pushes.len(), 1, "one push event: {seen:?}");
    assert_eq!(pushes[0].paths, vec!["src/a.txt".to_string()]);
    assert_eq!(pushes[0].detail["ref"], "refs/remotes/origin/main");
    assert_eq!(pushes[0].detail["new"], sha.as_str());
    stop(running, db).await
}

#[tokio::test]
async fn a_branch_taking_in_pushed_commits_records_only_its_own_commit() -> anyhow::Result<()> {
    let repo = repo().await?;
    let _remote = with_remote(&repo).await?;
    repo.git(&["branch", "side"]).await?;
    repo.write("src/pushed.txt", "already on the remote")?;
    repo.git(&["add", "src/pushed.txt"]).await?;
    repo.git(&["commit", "-q", "-m", "pushed work"]).await?;
    repo.git(&["push", "-q", "origin", "main"]).await?;
    let Some(db) = db(&repo.root).await? else {
        return Ok(());
    };
    let running = watching(&db, &repo, quick(true)).await?;
    repo.git(&["checkout", "-q", "side"]).await?;
    repo.git(&["merge", "-q", "--ff-only", "main"]).await?;
    repo.write("src/own.txt", "not pushed")?;
    repo.git(&["add", "src/own.txt"]).await?;
    repo.git(&["commit", "-q", "-m", "own work"]).await?;
    let seen = wait_for(&db, &repo.root, "the side branch's own commit", |e| {
        of_kind(e, ChangeKind::Commit)
            .iter()
            .any(|c| c.subject == "own work")
    })
    .await?;
    let subjects: Vec<&str> = of_kind(&seen, ChangeKind::Commit)
        .iter()
        .map(|c| c.subject.as_str())
        .collect();
    assert_eq!(subjects, vec!["own work"], "{seen:?}");
    stop(running, db).await
}

#[tokio::test]
async fn a_commit_names_the_fleet_seat_from_its_trailer() -> anyhow::Result<()> {
    let repo = repo().await?;
    let Some(db) = db(&repo.root).await? else {
        return Ok(());
    };
    let session_id = crate::store::persistence::change_record::test_seed::session(
        db.persist.pool(),
        db.project_id,
        Some(&repo.root),
    )
    .await?;
    let session = db
        .persist
        .session_identities(&[session_id])
        .await?
        .first()
        .map(|s| s.uuid)
        .ok_or_else(|| anyhow::anyhow!("seeded session {session_id} has no identity"))?;
    let running = watching(&db, &repo, quick(true)).await?;
    repo.write("src/b.txt", "by a seat")?;
    repo.git(&["add", "src/b.txt"]).await?;
    let message = format!("seat work\n\nFleet-Seat: rust-T2 ({session})\n");
    repo.git(&["commit", "-q", "-m", &message]).await?;
    let seen = wait_for(&db, &repo.root, "the named commit", |e| {
        of_kind(e, ChangeKind::Commit)
            .iter()
            .any(|c| c.session_id.is_some())
    })
    .await?;
    let commit = of_kind(&seen, ChangeKind::Commit)[0];
    assert_eq!(commit.session_id, Some(session_id));
    assert_eq!(commit.detail["author"], "fleet_seat_trailer");
    stop(running, db).await
}
