use serde_json::Value;

use super::Change;
use super::cause_steers_tests::Fixture;
use crate::store::persistence::web_repo::WebPersist as _;
use crate::store::persistence::wright::MergeOutcome;

async fn merged_into(fx: &Fixture, cause: i64) -> anyhow::Result<Option<i64>> {
    Ok(
        sqlx::query_scalar("SELECT merged_into FROM wright.causes WHERE id = $1")
            .bind(cause)
            .fetch_one(fx.db.handle.pool())
            .await?,
    )
}

async fn problems_of(fx: &Fixture, cause: i64) -> anyhow::Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT count(*) FROM wright.problems WHERE cause_id = $1")
            .bind(cause)
            .fetch_one(fx.db.handle.pool())
            .await?,
    )
}

#[tokio::test]
async fn merge_causes_merges_into_the_root_and_undo_separates_them() -> anyhow::Result<()> {
    let fx = Fixture::boot().await?;
    let [root, first, second] = [fx.causes[0], fx.causes[1], fx.causes[2]];
    let change = Change::MergeCauses {
        into: root,
        cause_ids: vec![first, second],
    };
    let applied = fx.run(&change).await?;
    assert_eq!(merged_into(&fx, first).await?, Some(root));
    assert_eq!(merged_into(&fx, second).await?, Some(root));
    assert_eq!(problems_of(&fx, root).await?, 3);
    assert_eq!(
        applied.effect["merge_ids"].as_array().map(Vec::len),
        Some(2)
    );
    let reason = fx.refusal(&Change::PinCause { cause_id: first }).await?;
    assert!(reason.contains("merged into another cause"), "{reason}");
    fx.undo(&change, &applied).await?;
    assert_eq!(merged_into(&fx, first).await?, None);
    assert_eq!(merged_into(&fx, second).await?, None);
    assert_eq!(problems_of(&fx, root).await?, 1);
    assert_eq!(problems_of(&fx, first).await?, 1);
    fx.done().await
}

#[tokio::test]
async fn merge_causes_undo_after_a_manual_separate_still_succeeds() -> anyhow::Result<()> {
    let fx = Fixture::boot().await?;
    let [root, first] = [fx.causes[0], fx.causes[1]];
    let change = Change::MergeCauses {
        into: root,
        cause_ids: vec![first],
    };
    let applied = fx.run(&change).await?;
    let merge_id = applied.effect["merge_ids"][0]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("no merge id in {}", applied.effect))?;
    fx.db
        .handle
        .web()
        .separate_wright_cause_merge(merge_id)
        .await?;
    let undone = fx.undo(&change, &applied).await?;
    assert_eq!(undone["separated"], Value::Array(Vec::new()));
    assert_eq!(merged_into(&fx, first).await?, None);
    fx.done().await
}

#[tokio::test]
async fn separate_merge_separates_an_operator_merge_and_undo_merges_it_again() -> anyhow::Result<()>
{
    let fx = Fixture::boot().await?;
    let [root, first] = [fx.causes[0], fx.causes[1]];
    let MergeOutcome::Merged(merged) = fx
        .db
        .handle
        .web()
        .merge_wright_causes_by_operator(root, &[first])
        .await?
    else {
        anyhow::bail!("the fixture merge did not run");
    };
    let change = Change::SeparateMerge {
        merge_id: merged.merge_ids[0],
    };
    let applied = fx.run(&change).await?;
    assert_eq!(merged_into(&fx, first).await?, None);
    assert_eq!(problems_of(&fx, first).await?, 1);
    fx.undo(&change, &applied).await?;
    assert_eq!(merged_into(&fx, first).await?, Some(root));
    assert_eq!(problems_of(&fx, root).await?, 2);
    fx.done().await
}

#[tokio::test]
async fn separate_merge_refuses_an_engineer_merge_and_a_separated_merge() -> anyhow::Result<()> {
    let fx = Fixture::boot().await?;
    let [root, first, second] = [fx.causes[0], fx.causes[1], fx.causes[2]];
    let problem: i64 = sqlx::query_scalar("SELECT id FROM wright.problems WHERE cause_id = $1")
        .bind(root)
        .fetch_one(fx.db.handle.pool())
        .await?;
    fx.db
        .handle
        .web()
        .merge_wright_causes(problem, &[first], "engineer")
        .await?;
    let engineer_merge: i64 =
        sqlx::query_scalar("SELECT id FROM wright.cause_merges WHERE by = 'engineer'")
            .fetch_one(fx.db.handle.pool())
            .await?;
    let reason = fx
        .refusal(&Change::SeparateMerge {
            merge_id: engineer_merge,
        })
        .await?;
    assert!(reason.contains("engineer"), "{reason}");
    let MergeOutcome::Merged(merged) = fx
        .db
        .handle
        .web()
        .merge_wright_causes_by_operator(root, &[second])
        .await?
    else {
        anyhow::bail!("the fixture merge did not run");
    };
    fx.db
        .handle
        .web()
        .separate_wright_cause_merge(merged.merge_ids[0])
        .await?;
    let reason = fx
        .refusal(&Change::SeparateMerge {
            merge_id: merged.merge_ids[0],
        })
        .await?;
    assert!(reason.contains("already separated"), "{reason}");
    fx.done().await
}
