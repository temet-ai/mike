use crate::store::persistence::fleet_repo::FleetPersist as _;
use chrono::Utc;
use sqlx::PgPool;

use super::{Caller, Scope, Since, changes, standing, tail, whose};
use crate::change_record::{ChangeKind, NewChangeEvent};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::change_record::test_seed::{self, CHECKOUT};
use crate::store::persistence::test_support::{self, TestDbHandle};

struct Fixture {
    db: TestDbHandle,
    handle: PersistHandle,
    project: i64,
}

async fn fixture() -> anyhow::Result<Option<Fixture>> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(None);
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = test_seed::project(handle.pool()).await?;
    Ok(Some(Fixture {
        db,
        handle,
        project,
    }))
}

async fn session(pool: &PgPool, project: i64) -> anyhow::Result<i64> {
    test_seed::session(pool, project, None).await
}

fn caller(session_id: i64, files: &[&str]) -> Caller {
    Caller {
        session_id,
        checkout: CHECKOUT.to_owned(),
        in_fleet: false,
        task_files: files.iter().map(|f| (*f).to_owned()).collect(),
    }
}

fn event(project: i64, kind: ChangeKind, session: Option<i64>, paths: &[&str]) -> NewChangeEvent {
    NewChangeEvent {
        project_id: project,
        checkout: CHECKOUT.to_owned(),
        at: Utc::now(),
        kind,
        session_id: session,
        run_id: None,
        task: None,
        paths: paths.iter().map(|p| (*p).to_owned()).collect(),
        subject: format!("{} {}", kind.as_str(), paths.join(",")),
        detail: serde_json::json!({}),
        before_sha: None,
        after_sha: (kind == ChangeKind::Commit).then(|| "abcdef1234567890".to_owned()),
    }
}

#[tokio::test]
async fn changes_returns_what_others_changed_in_my_paths_and_moves_the_place() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let me = session(pool, f.project).await?;
    let other = session(pool, f.project).await?;
    let events = vec![
        event(f.project, ChangeKind::Edit, Some(other), &["src/mine.rs"]),
        event(
            f.project,
            ChangeKind::Edit,
            Some(other),
            &["src/elsewhere.rs"],
        ),
        event(f.project, ChangeKind::Edit, Some(me), &["src/mine.rs"]),
        event(f.project, ChangeKind::Note, Some(other), &["src/mine.rs"]),
    ];
    f.handle.fleet().insert_change_events(&events).await?;

    let me_caller = caller(me, &["src/mine.rs"]);
    let first = changes(
        &f.handle,
        &me_caller,
        &Since::Time(Utc::now() - chrono::Duration::hours(1)),
        &Scope::Mine,
    )
    .await?;
    let hows: Vec<&str> = first.lines.iter().map(|l| l.how).collect();
    assert_eq!(hows, vec!["edited", "note"], "{first:?}");
    assert!(first.lines.iter().all(|l| l.paths == vec!["src/mine.rs"]));
    assert!(first.place > 0);

    let again = changes(&f.handle, &me_caller, &Since::Place, &Scope::Mine).await?;
    assert!(again.lines.is_empty(), "{again:?}");

    f.handle
        .fleet()
        .insert_change_events(&[event(
            f.project,
            ChangeKind::Edit,
            Some(other),
            &["src/mine.rs"],
        )])
        .await?;
    let third = changes(&f.handle, &me_caller, &Since::Place, &Scope::Mine).await?;
    assert_eq!(third.lines.len(), 1, "{third:?}");
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn changes_for_all_paths_and_for_a_named_path_and_an_unknown_commit_errs()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let me = session(pool, f.project).await?;
    let other = session(pool, f.project).await?;
    f.handle
        .fleet()
        .insert_change_events(&[
            event(f.project, ChangeKind::Edit, Some(other), &["a.rs"]),
            event(f.project, ChangeKind::Commit, Some(other), &["b.rs"]),
        ])
        .await?;
    let c = caller(me, &[]);
    let since = Since::Time(Utc::now() - chrono::Duration::hours(1));
    let named = changes(
        &f.handle,
        &c,
        &since,
        &Scope::Paths(vec!["b.rs".to_owned()]),
    )
    .await?;
    assert_eq!(named.lines.len(), 1);
    assert_eq!(named.lines[0].commit.as_deref(), Some("abcdef12"));
    let all = changes(&f.handle, &c, &since, &Scope::All).await?;
    assert_eq!(all.lines.len(), 2);
    let missing = changes(
        &f.handle,
        &c,
        &Since::Commit("ffff".to_owned()),
        &Scope::All,
    )
    .await;
    assert!(missing.is_err());
    let by_commit = changes(
        &f.handle,
        &caller(me, &[]),
        &Since::Commit("abcdef12".to_owned()),
        &Scope::All,
    )
    .await?;
    assert!(by_commit.lines.is_empty());
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn tail_leaves_out_changes_no_author_is_known_for() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let me = session(pool, f.project).await?;
    let other = session(pool, f.project).await?;
    let c = caller(me, &["src"]);
    let seed = event(f.project, ChangeKind::Edit, Some(other), &["src/seed.rs"]);
    f.handle.fleet().insert_change_events(&[seed]).await?;
    tail(&f.handle, &c).await?;
    let anonymous = event(f.project, ChangeKind::Edit, None, &["src/nobody.rs"]);
    f.handle.fleet().insert_change_events(&[anonymous]).await?;
    assert!(tail(&f.handle, &c).await?.is_empty());
    let known = event(f.project, ChangeKind::Edit, Some(other), &["src/known.rs"]);
    f.handle.fleet().insert_change_events(&[known]).await?;
    let lines = tail(&f.handle, &c).await?;
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains("src/known.rs"));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn tail_is_empty_when_nothing_changed_and_holds_to_twelve_lines() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let me = session(pool, f.project).await?;
    let other = session(pool, f.project).await?;
    let c = caller(me, &["src"]);
    assert!(tail(&f.handle, &c).await?.is_empty());

    f.handle
        .fleet()
        .insert_change_events(&[event(
            f.project,
            ChangeKind::Edit,
            Some(other),
            &["src/a.rs"],
        )])
        .await?;
    let first = tail(&f.handle, &c).await?;
    assert!(
        first.is_empty(),
        "the first look only sets the place: {first:?}"
    );

    let many: Vec<NewChangeEvent> = (0..30)
        .map(|n| {
            event(
                f.project,
                ChangeKind::Edit,
                Some(other),
                &[&format!("src/f{n}.rs")],
            )
        })
        .collect();
    f.handle.fleet().insert_change_events(&many).await?;
    let lines = tail(&f.handle, &c).await?;
    assert_eq!(lines.len(), 12, "{lines:?}");
    assert!(lines[11].contains("what_changed"));

    let rest = tail(&f.handle, &c).await?;
    assert!(rest.len() <= 12);
    assert!(rest.iter().any(|l| l.contains("src/f")), "{rest:?}");
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn whose_names_the_last_changer_and_whether_it_is_committed() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let other = session(pool, f.project).await?;
    let none = whose(&f.handle, CHECKOUT, "a.rs").await?;
    assert!(none.last_change.is_none() && !none.committed);

    f.handle
        .fleet()
        .insert_change_events(&[event(f.project, ChangeKind::Edit, Some(other), &["a.rs"])])
        .await?;
    let edited = whose(&f.handle, CHECKOUT, "a.rs").await?;
    assert_eq!(edited.last_change.as_ref().map(|l| l.how), Some("edited"));
    assert!(!edited.committed);

    f.handle
        .fleet()
        .insert_change_events(&[event(f.project, ChangeKind::Commit, Some(other), &["a.rs"])])
        .await?;
    assert!(whose(&f.handle, CHECKOUT, "a.rs").await?.committed);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn standing_outside_a_fleet_lists_other_sessions_and_the_newest_commits() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let me = session(pool, f.project).await?;
    let other = session(pool, f.project).await?;
    f.handle
        .fleet()
        .insert_change_events(&[
            event(f.project, ChangeKind::Edit, Some(me), &["mine.rs"]),
            event(f.project, ChangeKind::Edit, Some(other), &["theirs.rs"]),
            event(f.project, ChangeKind::Commit, Some(other), &["old.rs"]),
        ])
        .await?;
    let answer = standing(&f.handle, &caller(me, &[])).await?;
    assert_eq!(answer.uncommitted, vec!["mine.rs".to_owned()]);
    let others = answer.others.unwrap_or_default();
    assert_eq!(others.len(), 1);
    assert_eq!(others[0].uncommitted, vec!["theirs.rs".to_owned()]);
    assert_eq!(answer.newest_commits.unwrap_or_default().len(), 1);
    assert_eq!(answer.events_waiting, 2);

    let mut fleet_caller = caller(me, &[]);
    fleet_caller.in_fleet = true;
    let seat = standing(&f.handle, &fleet_caller).await?;
    assert!(seat.others.is_none() && seat.newest_commits.is_none());
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn whose_commit_without_a_later_edit_is_committed_and_an_anonymous_line_says_so()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    f.handle
        .fleet()
        .insert_change_events(&[
            event(f.project, ChangeKind::Commit, None, &["b.rs"]),
            event(f.project, ChangeKind::Edit, None, &["c.rs"]),
        ])
        .await?;
    assert!(whose(&f.handle, CHECKOUT, "b.rs").await?.committed);
    let anonymous = whose(&f.handle, CHECKOUT, "c.rs").await?;
    assert!(!anonymous.committed);
    assert_eq!(
        anonymous.last_change.map(|l| l.who).as_deref(),
        Some("no author known")
    );
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn whose_path_with_no_event_follows_git_at_the_checkout() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let dir = tempfile::tempdir()?;
    let root = dir.path().to_string_lossy().into_owned();
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["-c", "user.email=t@t", "-c", "user.name=t"])
            .args(args)
            .output()
    };
    git(&["init", "-q"])?;
    std::fs::write(dir.path().join("kept.txt"), "one")?;
    std::fs::write(dir.path().join("dirty.txt"), "one")?;
    git(&["add", "."])?;
    git(&["commit", "-q", "-m", "x"])?;
    std::fs::write(dir.path().join("dirty.txt"), "two")?;
    assert!(whose(&f.handle, &root, "kept.txt").await?.committed);
    assert!(!whose(&f.handle, &root, "dirty.txt").await?.committed);
    assert!(!whose(&f.handle, &root, "never.txt").await?.committed);
    f.db.cleanup().await?;
    Ok(())
}
