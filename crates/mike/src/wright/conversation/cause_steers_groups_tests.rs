use super::cause_steers_tests::Fixture;
use super::{Change, SteerError};
use crate::store::persistence::web_repo::WebPersist as _;
use crate::store::persistence::wright::{CausePatch, GroupWrite};

async fn group_names(fx: &Fixture) -> anyhow::Result<Vec<String>> {
    Ok(fx
        .db
        .handle
        .web()
        .list_wright_cause_groups()
        .await?
        .into_iter()
        .map(|group| group.name)
        .collect())
}

async fn make_group(fx: &Fixture, name: &str) -> anyhow::Result<i64> {
    match fx.db.handle.web().create_wright_cause_group(name).await? {
        GroupWrite::Saved(group) => Ok(group.id),
        other => anyhow::bail!("the fixture group was not created: {other:?}"),
    }
}

async fn put_in(fx: &Fixture, cause: i64, group: Option<i64>) -> anyhow::Result<()> {
    fx.patch(
        cause,
        CausePatch {
            group_id: Some(group),
            ..CausePatch::default()
        },
    )
    .await
}

#[tokio::test]
async fn group_causes_creates_the_group_and_undo_deletes_it_once_empty() -> anyhow::Result<()> {
    let fx = Fixture::boot().await?;
    let [first, second] = [fx.causes[0], fx.causes[1]];
    let change = Change::GroupCauses {
        name: "slow memory queries".to_owned(),
        cause_ids: vec![first, second],
    };
    let applied = fx.run(&change).await?;
    let group = fx.row(first).await?.group_id;
    assert!(group.is_some());
    assert_eq!(fx.row(second).await?.group_id, group);
    assert_eq!(
        group_names(&fx).await?,
        vec!["slow memory queries".to_owned()]
    );
    assert_eq!(applied.effect["group_created"], true);
    fx.undo(&change, &applied).await?;
    assert_eq!(fx.row(first).await?.group_id, None);
    assert_eq!(fx.row(second).await?.group_id, None);
    assert!(group_names(&fx).await?.is_empty());
    fx.done().await
}

#[tokio::test]
async fn group_causes_reuses_a_group_and_undo_restores_the_previous_group() -> anyhow::Result<()> {
    let fx = Fixture::boot().await?;
    let [first, second, third] = [fx.causes[0], fx.causes[1], fx.causes[2]];
    let existing = make_group(&fx, "Slow Memory Queries").await?;
    let old = make_group(&fx, "old home").await?;
    put_in(&fx, first, Some(existing)).await?;
    put_in(&fx, third, Some(old)).await?;
    let change = Change::GroupCauses {
        name: "slow memory queries".to_owned(),
        cause_ids: vec![second, third],
    };
    let applied = fx.run(&change).await?;
    assert_eq!(applied.effect["group_created"], false);
    assert_eq!(fx.row(second).await?.group_id, Some(existing));
    assert_eq!(fx.row(third).await?.group_id, Some(existing));
    assert_eq!(group_names(&fx).await?.len(), 2);
    fx.undo(&change, &applied).await?;
    assert_eq!(fx.row(first).await?.group_id, Some(existing));
    assert_eq!(fx.row(second).await?.group_id, None);
    assert_eq!(fx.row(third).await?.group_id, Some(old));
    assert_eq!(group_names(&fx).await?.len(), 2);
    fx.done().await
}

#[tokio::test]
async fn group_causes_undo_keeps_a_created_group_that_gained_another_cause() -> anyhow::Result<()> {
    let fx = Fixture::boot().await?;
    let [first, second] = [fx.causes[0], fx.causes[1]];
    let change = Change::GroupCauses {
        name: "fresh".to_owned(),
        cause_ids: vec![first],
    };
    let applied = fx.run(&change).await?;
    let group = fx.row(first).await?.group_id;
    put_in(&fx, second, group).await?;
    fx.undo(&change, &applied).await?;
    assert_eq!(fx.row(first).await?.group_id, None);
    assert_eq!(fx.row(second).await?.group_id, group);
    assert_eq!(group_names(&fx).await?, vec!["fresh".to_owned()]);
    fx.done().await
}

#[tokio::test]
async fn group_causes_undo_after_a_cause_moved_groups_is_a_conflict() -> anyhow::Result<()> {
    let fx = Fixture::boot().await?;
    let first = fx.causes[0];
    let change = Change::GroupCauses {
        name: "fresh".to_owned(),
        cause_ids: vec![first],
    };
    let applied = fx.run(&change).await?;
    let elsewhere = make_group(&fx, "elsewhere").await?;
    put_in(&fx, first, Some(elsewhere)).await?;
    let outcome = fx.undo(&change, &applied).await;
    assert!(
        matches!(outcome, Err(SteerError::Conflict(_))),
        "{outcome:?}"
    );
    assert_eq!(fx.row(first).await?.group_id, Some(elsewhere));
    fx.done().await
}

#[tokio::test]
async fn ungroup_causes_removes_the_groups_and_undo_puts_each_cause_back() -> anyhow::Result<()> {
    let fx = Fixture::boot().await?;
    let [first, second, third] = [fx.causes[0], fx.causes[1], fx.causes[2]];
    let left = make_group(&fx, "left").await?;
    let right = make_group(&fx, "right").await?;
    put_in(&fx, first, Some(left)).await?;
    put_in(&fx, second, Some(right)).await?;
    let change = Change::UngroupCauses {
        cause_ids: vec![first, second, third],
    };
    let applied = fx.run(&change).await?;
    for cause in [first, second, third] {
        assert_eq!(fx.row(cause).await?.group_id, None);
    }
    fx.undo(&change, &applied).await?;
    assert_eq!(fx.row(first).await?.group_id, Some(left));
    assert_eq!(fx.row(second).await?.group_id, Some(right));
    assert_eq!(fx.row(third).await?.group_id, None);
    fx.done().await
}

#[tokio::test]
async fn ungroup_causes_undo_after_the_group_was_deleted_is_a_conflict() -> anyhow::Result<()> {
    let fx = Fixture::boot().await?;
    let first = fx.causes[0];
    let group = make_group(&fx, "doomed").await?;
    put_in(&fx, first, Some(group)).await?;
    let change = Change::UngroupCauses {
        cause_ids: vec![first],
    };
    let applied = fx.run(&change).await?;
    fx.db.handle.web().delete_wright_cause_group(group).await?;
    let outcome = fx.undo(&change, &applied).await;
    assert!(
        matches!(outcome, Err(SteerError::Conflict(_))),
        "{outcome:?}"
    );
    assert_eq!(fx.row(first).await?.group_id, None);
    fx.done().await
}
