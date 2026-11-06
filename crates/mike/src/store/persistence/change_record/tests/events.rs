use chrono::Duration;
use serde_json::json;

use super::fixture;
use super::seed::{CHECKOUT, event, project, session};
use crate::change_record::{ChangeKind, ChangeQuery};

#[tokio::test]
async fn insert_change_events_returns_ids_in_order_and_round_trips_every_field()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let project_id = project(pool).await?;
    let author = session(pool, project_id, None).await?;
    let mut first = event(project_id, ChangeKind::Edit, &["src/a.rs", "src/b.rs"]);
    first.session_id = Some(author);
    first.run_id = Some("run-1".to_owned());
    first.task = Some("T1".to_owned());
    first.detail = json!({"tool_use_id": 7});
    first.before_sha = Some("a".repeat(40));
    first.after_sha = Some("b".repeat(40));
    let second = event(project_id, ChangeKind::Note, &[]);
    first.at -= Duration::seconds(5);

    let ids = f
        .handle
        .insert_change_events(&[first.clone(), second])
        .await?;
    let [a, b] = ids.as_slice() else {
        anyhow::bail!("expected two ids, got {ids:?}");
    };
    assert!(a < b);

    let rows = f
        .handle
        .change_events_after(&ChangeQuery::everything_after(CHECKOUT, 0, 10))
        .await?;
    let [row_a, row_b] = rows.as_slice() else {
        anyhow::bail!("expected two rows, got {}", rows.len());
    };
    assert_eq!(
        row_a.paths,
        vec!["src/a.rs".to_owned(), "src/b.rs".to_owned()]
    );
    assert_eq!(row_a.session_id, Some(author));
    assert_eq!(row_a.task.as_deref(), Some("T1"));
    assert_eq!(row_a.detail, json!({"tool_use_id": 7}));
    assert_eq!(row_a.before_sha, first.before_sha);
    assert_eq!(row_a.kind, ChangeKind::Edit);
    assert!(row_b.paths.is_empty());
    assert_eq!(row_b.session_id, None);
    assert_eq!(row_b.kind, ChangeKind::Note);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn insert_change_events_empty_batch_writes_nothing() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    assert!(f.handle.insert_change_events(&[]).await?.is_empty());
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn insert_change_events_unknown_project_is_an_error_and_writes_nothing() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let project_id = project(f.handle.pool()).await?;
    let good = event(project_id, ChangeKind::Edit, &["a"]);
    let bad = event(project_id + 999, ChangeKind::Edit, &["b"]);
    assert!(f.handle.insert_change_events(&[good, bad]).await.is_err());
    let rows = f
        .handle
        .change_events_after(&ChangeQuery::everything_after(CHECKOUT, 0, 10))
        .await?;
    assert!(rows.is_empty());
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn insert_change_events_path_with_separator_is_refused() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let project_id = project(f.handle.pool()).await?;
    let bad = event(project_id, ChangeKind::Edit, &["a\u{1f}b"]);
    assert!(f.handle.insert_change_events(&[bad]).await.is_err());
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn change_events_after_filters_by_checkout_id_prefix_session_kind_and_limit()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let project_id = project(pool).await?;
    let mine = session(pool, project_id, None).await?;
    let other = session(pool, project_id, None).await?;
    let mut events = vec![
        event(project_id, ChangeKind::Edit, &["crates/a.rs"]),
        event(project_id, ChangeKind::Edit, &["docs/x.md", "crates/b.rs"]),
        event(project_id, ChangeKind::Commit, &["crates/a.rs"]),
        event(project_id, ChangeKind::Edit, &["scripts/cx"]),
    ];
    events[0].session_id = Some(mine);
    events[1].session_id = Some(other);
    events[3].session_id = Some(other);
    let mut elsewhere = event(project_id, ChangeKind::Edit, &["crates/a.rs"]);
    elsewhere.checkout = "/work/beta".to_owned();
    events.push(elsewhere);
    let ids = f.handle.insert_change_events(&events).await?;

    let all = ChangeQuery::everything_after(CHECKOUT, 0, 100);
    assert_eq!(f.handle.change_events_after(&all).await?.len(), 4);

    let after = ChangeQuery::everything_after(CHECKOUT, ids[1], 100);
    let rows = f.handle.change_events_after(&after).await?;
    assert_eq!(
        rows.iter().map(|r| r.id).collect::<Vec<_>>(),
        vec![ids[2], ids[3]]
    );

    let prefixed = ChangeQuery {
        path_prefixes: vec!["crates/".to_owned(), "nothing/".to_owned()],
        ..all.clone()
    };
    let rows = f.handle.change_events_after(&prefixed).await?;
    assert_eq!(
        rows.iter().map(|r| r.id).collect::<Vec<_>>(),
        vec![ids[0], ids[1], ids[2]]
    );

    let not_mine = ChangeQuery {
        exclude_session: Some(mine),
        ..all.clone()
    };
    let rows = f.handle.change_events_after(&not_mine).await?;
    assert_eq!(
        rows.iter().map(|r| r.id).collect::<Vec<_>>(),
        vec![ids[1], ids[2], ids[3]]
    );

    let commits = ChangeQuery {
        kinds: vec![ChangeKind::Commit],
        ..all.clone()
    };
    assert_eq!(f.handle.change_events_after(&commits).await?.len(), 1);

    let limited = ChangeQuery { limit: 2, ..all };
    assert_eq!(f.handle.change_events_after(&limited).await?.len(), 2);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn update_change_author_names_session_kind_and_merges_detail() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let project_id = project(pool).await?;
    let author = session(pool, project_id, None).await?;
    let mut new = event(project_id, ChangeKind::ShellChange, &["a.rs"]);
    new.detail = json!({"candidates": [1, 2]});
    let ids = f.handle.insert_change_events(&[new]).await?;

    let named = f
        .handle
        .update_change_author(
            ids[0],
            Some(author),
            ChangeKind::Edit,
            &json!({"tool_use_id": 9}),
        )
        .await?;
    assert!(named);
    let rows = f
        .handle
        .change_events_after(&ChangeQuery::everything_after(CHECKOUT, 0, 10))
        .await?;
    assert_eq!(rows[0].session_id, Some(author));
    assert_eq!(rows[0].kind, ChangeKind::Edit);
    assert_eq!(
        rows[0].detail,
        json!({"candidates": [1, 2], "tool_use_id": 9})
    );

    let missing = f
        .handle
        .update_change_author(ids[0] + 100, None, ChangeKind::Edit, &json!({}))
        .await?;
    assert!(!missing);
    let no_such_session = f
        .handle
        .update_change_author(ids[0], Some(author + 100), ChangeKind::Edit, &json!({}))
        .await;
    assert!(no_such_session.is_err());
    f.db.cleanup().await?;
    Ok(())
}
