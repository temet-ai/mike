use super::{Repo, db, quick, repo, stop, wait_for, watching};
use crate::change_record::{ChangeEvent, ChangeKind};
use crate::store::persistence::change_record::test_seed::session;

async fn with_remote(repo: &Repo) -> anyhow::Result<tempfile::TempDir> {
    let remote = tempfile::tempdir()?;
    let url = remote.path().to_string_lossy().into_owned();
    repo.git(&["init", "-q", "--bare", &url]).await?;
    repo.git(&["remote", "add", "origin", &url]).await?;
    repo.git(&["push", "-q", "origin", "main"]).await?;
    Ok(remote)
}

fn named_pushes(events: &[ChangeEvent]) -> Vec<&ChangeEvent> {
    events
        .iter()
        .filter(|e| e.kind == ChangeKind::Push && e.detail["author"] != "pending")
        .collect()
}

async fn seat_commit(repo: &Repo, path: &str, seat: uuid::Uuid) -> anyhow::Result<()> {
    repo.write(path, "by a seat")?;
    repo.git(&["add", path]).await?;
    let message = format!("seat work\n\nFleet-Seat: rust-T1 ({seat})\n");
    repo.git(&["commit", "-q", "-m", &message]).await?;
    Ok(())
}

async fn uuid_of(db: &super::Db, id: i64) -> anyhow::Result<uuid::Uuid> {
    db.persist
        .session_identities(&[id])
        .await?
        .first()
        .map(|s| s.uuid)
        .ok_or_else(|| anyhow::anyhow!("seeded session {id} has no identity"))
}

#[tokio::test]
async fn a_push_of_one_seats_commits_names_that_seat() -> anyhow::Result<()> {
    let repo = repo().await?;
    let _remote = with_remote(&repo).await?;
    let Some(db) = db(&repo.root).await? else {
        return Ok(());
    };
    let seat = session(db.persist.pool(), db.project_id, Some(&repo.root)).await?;
    let running = watching(&db, &repo, quick(true)).await?;
    seat_commit(&repo, "src/b.txt", uuid_of(&db, seat).await?).await?;
    repo.git(&["push", "-q", "origin", "main"]).await?;
    let seen = wait_for(&db, &repo.root, "the named push", |e| {
        !named_pushes(e).is_empty()
    })
    .await?;
    let push = named_pushes(&seen)[0];
    assert_eq!(push.session_id, Some(seat), "{push:?}");
    assert_eq!(push.detail["author"], "fleet_seat_trailer");
    stop(running, db).await
}

#[tokio::test]
async fn a_push_of_two_seats_commits_names_no_author() -> anyhow::Result<()> {
    let repo = repo().await?;
    let _remote = with_remote(&repo).await?;
    let Some(db) = db(&repo.root).await? else {
        return Ok(());
    };
    let one = session(db.persist.pool(), db.project_id, Some(&repo.root)).await?;
    let two = session(db.persist.pool(), db.project_id, Some(&repo.root)).await?;
    let running = watching(&db, &repo, quick(true)).await?;
    seat_commit(&repo, "src/b.txt", uuid_of(&db, one).await?).await?;
    seat_commit(&repo, "src/c.txt", uuid_of(&db, two).await?).await?;
    repo.git(&["push", "-q", "origin", "main"]).await?;
    let seen = wait_for(&db, &repo.root, "the unnamed push", |e| {
        !named_pushes(e).is_empty()
    })
    .await?;
    let push = named_pushes(&seen)[0];
    assert_eq!(push.session_id, None, "{push:?}");
    assert_eq!(push.detail["author"], "unknown");
    stop(running, db).await
}
