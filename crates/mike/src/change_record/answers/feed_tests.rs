use super::feed;
use crate::change_record::ChangeKind;
use crate::change_record::answers::{Caller, tail};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::change_record::test_seed::{self, CHECKOUT};
use crate::store::persistence::test_support;

#[tokio::test]
async fn feed_lists_every_author_newest_first_and_leaves_the_place_alone() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = handle.pool();
    let project = test_seed::project(pool).await?;
    let me = test_seed::session(pool, project, None).await?;
    let other = test_seed::session(pool, project, None).await?;
    let caller = Caller {
        session_id: me,
        checkout: CHECKOUT.to_owned(),
        in_fleet: false,
        task_files: Vec::new(),
    };

    let mut seen = test_seed::event(project, ChangeKind::Edit, &["src/seen.rs"]);
    seen.session_id = Some(other);
    handle.insert_change_events(&[seen]).await?;
    assert!(tail(&handle, &caller).await?.is_empty());
    let place = handle.change_place(me, CHECKOUT).await?;

    let mut mine = test_seed::event(project, ChangeKind::Edit, &["src/mine.rs"]);
    mine.session_id = Some(me);
    let unknown = test_seed::event(project, ChangeKind::ShellChange, &["src/by_hand.rs"]);
    handle.insert_change_events(&[mine]).await?;
    handle.insert_change_events(&[unknown]).await?;

    let answer = feed(&handle, CHECKOUT, 10).await?;
    let paths: Vec<&str> = answer
        .lines
        .iter()
        .flat_map(|l| l.paths.iter().map(String::as_str))
        .collect();
    assert_eq!(paths, vec!["src/by_hand.rs", "src/mine.rs", "src/seen.rs"]);
    assert_eq!(
        answer.lines.first().map(|l| l.who.as_str()),
        Some("no author known")
    );
    let own = answer.lines.iter().find(|l| l.paths == ["src/mine.rs"]);
    let me_uuid = handle
        .session_identities(&[me])
        .await?
        .first()
        .map(|i| i.uuid);
    assert_eq!(own.and_then(|l| l.session), me_uuid);
    assert_eq!(answer.lines.first().and_then(|l| l.session), None);
    assert_eq!(handle.change_place(me, CHECKOUT).await?, place);

    let limited = feed(&handle, CHECKOUT, 1).await?;
    assert_eq!(limited.lines.len(), 1);
    db.cleanup().await?;
    Ok(())
}
