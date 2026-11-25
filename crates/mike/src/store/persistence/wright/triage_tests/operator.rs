use super::{CHECK, Env};
use crate::store::persistence::web_repo::WebPersist as _;
use crate::store::persistence::wright::{
    CausePatch, GroupWrite, MergeOutcome, PatchOutcome, SeparateOutcome,
};

async fn shared_and_lone(env: &Env) -> anyhow::Result<(i64, i64, i64, i64)> {
    let run = env.run(CHECK).await?;
    let (lone, _) = env.problem(CHECK, "query 1: SELECT lone", run, 30).await?;
    let mut shared = 0;
    for n in 0..4 {
        let (problem, _) = env
            .problem(
                CHECK,
                &format!("query {n}9: WITH shared AS (SELECT 1)"),
                run,
                1,
            )
            .await?;
        shared = problem;
    }
    env.persist.web().assign_wright_causes().await?;
    Ok((
        env.cause(lone).await?,
        env.cause(shared).await?,
        lone,
        shared,
    ))
}

#[tokio::test]
async fn a_pinned_cause_comes_first_in_the_list_and_the_pick_despite_a_lower_score()
-> anyhow::Result<()> {
    let env = Env::boot().await?;
    let (lone_cause, shared_cause, lone, _) = shared_and_lone(&env).await?;
    assert_eq!(env.queue_ids().await?, vec![shared_cause, lone_cause]);
    let outcome = env
        .patch(
            lone_cause,
            CausePatch {
                pinned: Some(true),
                ..CausePatch::default()
            },
        )
        .await?;
    let PatchOutcome::Updated(row) = outcome else {
        anyhow::bail!("the pin was not applied: {outcome:?}");
    };
    assert!(row.pinned && row.pinned_at.is_some());
    assert!(row.rank_priority < env.row(shared_cause).await?.rank_priority);
    assert_eq!(env.queue_ids().await?, vec![lone_cause, shared_cause]);
    let pick = env.persist.web().pick_wright_problem(2).await?;
    assert_eq!(pick.map(|p| p.problem_id), Some(lone));
    let unpinned = env
        .patch(
            lone_cause,
            CausePatch {
                pinned: Some(false),
                ..CausePatch::default()
            },
        )
        .await?;
    assert!(matches!(unpinned, PatchOutcome::Updated(row) if !row.pinned));
    assert_eq!(env.queue_ids().await?, vec![shared_cause, lone_cause]);
    env.done().await
}

#[tokio::test]
async fn an_operator_priority_replaces_the_score_and_clearing_it_restores_the_order()
-> anyhow::Result<()> {
    let env = Env::boot().await?;
    let (lone_cause, shared_cause, _, shared) = shared_and_lone(&env).await?;
    let set = |value: Option<f64>| CausePatch {
        operator_priority: Some(value),
        ..CausePatch::default()
    };
    env.patch(lone_cause, set(Some(1000.0))).await?;
    let row = env.row(lone_cause).await?;
    assert_eq!(row.operator_priority, Some(1000.0));
    assert!((row.rank_priority - 1000.0).abs() < f64::EPSILON);
    assert!(row.priority < 100.0);
    assert_eq!(env.queue_ids().await?, vec![lone_cause, shared_cause]);
    let pick = env.persist.web().pick_wright_problem(2).await?;
    assert_ne!(pick.map(|p| p.problem_id), Some(shared));
    env.patch(lone_cause, set(None)).await?;
    assert_eq!(env.row(lone_cause).await?.operator_priority, None);
    assert_eq!(env.queue_ids().await?, vec![shared_cause, lone_cause]);
    env.done().await
}

#[tokio::test]
async fn a_patch_of_an_unknown_cause_or_group_changes_nothing() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let (lone_cause, _, _, _) = shared_and_lone(&env).await?;
    let pin = CausePatch {
        pinned: Some(true),
        ..CausePatch::default()
    };
    assert_eq!(
        env.patch(lone_cause + 1000, pin).await?,
        PatchOutcome::NoCause
    );
    let group = CausePatch {
        group_id: Some(Some(424_242)),
        ..CausePatch::default()
    };
    assert_eq!(env.patch(lone_cause, group).await?, PatchOutcome::NoGroup);
    assert!(!env.row(lone_cause).await?.pinned);
    env.done().await
}

#[tokio::test]
async fn an_operator_merge_then_separate_restores_the_problems_and_keeps_later_ones()
-> anyhow::Result<()> {
    let env = Env::boot().await?;
    let run = env.run(CHECK).await?;
    let (a, _) = env.problem(CHECK, "query 1: SELECT a", run, 1).await?;
    let (b, _) = env.problem(CHECK, "query 2: SELECT b", run, 1).await?;
    let (c, _) = env.problem(CHECK, "query 3: SELECT c", run, 1).await?;
    env.persist.web().assign_wright_causes().await?;
    let (ca, cb, cc) = (
        env.cause(a).await?,
        env.cause(b).await?,
        env.cause(c).await?,
    );
    let merge_ids = env.operator_merge(ca, &[cb, cc]).await?;
    assert_eq!(merge_ids.len(), 2);
    assert_eq!((env.cause(b).await?, env.cause(c).await?), (ca, ca));
    let (later, _) = env.problem(CHECK, "query 9: SELECT b", run, 1).await?;
    env.persist.web().assign_wright_causes().await?;
    assert_eq!(env.cause(later).await?, ca);
    let row = env.row(ca).await?;
    assert_eq!(row.problems, 4);
    assert_eq!(row.absorbed.0.len(), 2);
    assert!(
        row.absorbed
            .0
            .iter()
            .all(|a| a.by == "operator" && a.can_separate)
    );
    assert_eq!(row.absorbed.0.iter().map(|a| a.problems).sum::<i64>(), 2);
    let cb_merge = row
        .absorbed
        .0
        .iter()
        .find(|a| a.cause_id == cb)
        .map(|a| a.merge_id)
        .ok_or_else(|| anyhow::anyhow!("no merge row names cause {cb}"))?;
    let outcome = env
        .persist
        .web()
        .separate_wright_cause_merge(cb_merge)
        .await?;
    let SeparateOutcome::Separated(merge) = outcome else {
        anyhow::bail!("the merge was not separated: {outcome:?}");
    };
    assert_eq!((merge.absorbed_cause_id, merge.problem_ids), (cb, vec![b]));
    assert!(merge.separated_at.is_some());
    assert_eq!(env.cause(b).await?, cb);
    assert_eq!(env.cause(c).await?, ca);
    assert_eq!(env.cause(later).await?, ca);
    assert_eq!(env.row(ca).await?.absorbed.0.len(), 1);
    assert!(env.queue_ids().await?.contains(&cb));
    let again = env
        .persist
        .web()
        .separate_wright_cause_merge(cb_merge)
        .await?;
    assert_eq!(again, SeparateOutcome::AlreadySeparated);
    env.done().await
}

#[tokio::test]
async fn separating_a_cause_gives_back_the_causes_it_had_absorbed() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let run = env.run(CHECK).await?;
    let (a, _) = env.problem(CHECK, "query 1: SELECT a", run, 1).await?;
    let (b, _) = env.problem(CHECK, "query 2: SELECT b", run, 1).await?;
    let (c, _) = env.problem(CHECK, "query 3: SELECT c", run, 1).await?;
    env.persist.web().assign_wright_causes().await?;
    let (ca, cb, cc) = (
        env.cause(a).await?,
        env.cause(b).await?,
        env.cause(c).await?,
    );
    env.operator_merge(ca, &[cb]).await?;
    let into_c = env.operator_merge(cc, &[ca]).await?;
    assert_eq!(env.cause(a).await?, cc);
    assert_eq!(env.cause(b).await?, cc);
    let outcome = env
        .persist
        .web()
        .separate_wright_cause_merge(into_c[0])
        .await?;
    assert!(matches!(outcome, SeparateOutcome::Separated(_)));
    assert_eq!(env.cause(a).await?, ca);
    assert_eq!(env.cause(b).await?, ca);
    assert_eq!(env.cause(c).await?, cc);
    env.done().await
}

#[tokio::test]
async fn separate_refuses_an_engineer_merge_and_an_unknown_one() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let run = env.run(CHECK).await?;
    let (root, _) = env.problem(CHECK, "query 1: SELECT a", run, 1).await?;
    let (other, _) = env.problem(CHECK, "query 2: SELECT b", run, 1).await?;
    env.persist.web().assign_wright_causes().await?;
    let other_cause = env.cause(other).await?;
    env.persist
        .web()
        .merge_wright_causes(root, &[other_cause], "engineer")
        .await?;
    let merge_id: i64 = sqlx::query_scalar("SELECT id FROM wright.cause_merges")
        .fetch_one(env.persist.pool())
        .await?;
    let refused = env
        .persist
        .web()
        .separate_wright_cause_merge(merge_id)
        .await?;
    assert_eq!(refused, SeparateOutcome::EngineerMerge);
    assert_eq!(env.cause(other).await?, env.cause(root).await?);
    let unknown = env
        .persist
        .web()
        .separate_wright_cause_merge(merge_id + 1000)
        .await?;
    assert_eq!(unknown, SeparateOutcome::NotFound);
    let row = env.row(env.cause(root).await?).await?;
    assert_eq!(row.absorbed.0.len(), 1);
    assert!(!row.absorbed.0[0].can_separate);
    env.done().await
}

#[tokio::test]
async fn an_operator_merge_refuses_a_merged_away_or_unknown_cause() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let run = env.run(CHECK).await?;
    let (a, _) = env.problem(CHECK, "query 1: SELECT a", run, 1).await?;
    let (b, _) = env.problem(CHECK, "query 2: SELECT b", run, 1).await?;
    env.persist.web().assign_wright_causes().await?;
    let (ca, cb) = (env.cause(a).await?, env.cause(b).await?);
    env.operator_merge(ca, &[cb]).await?;
    let web = env.persist.web();
    assert_eq!(
        web.merge_wright_causes_by_operator(cb, &[ca]).await?,
        MergeOutcome::NoRoot
    );
    assert_eq!(
        web.merge_wright_causes_by_operator(ca, &[cb]).await?,
        MergeOutcome::Unknown(vec![cb])
    );
    assert_eq!(
        web.merge_wright_causes_by_operator(ca, &[ca + 1000])
            .await?,
        MergeOutcome::Unknown(vec![ca + 1000])
    );
    env.done().await
}

#[tokio::test]
async fn deleting_a_group_clears_it_from_its_causes_and_a_duplicate_name_is_refused()
-> anyhow::Result<()> {
    let env = Env::boot().await?;
    let (lone_cause, shared_cause, _, _) = shared_and_lone(&env).await?;
    let web = env.persist.web();
    let GroupWrite::Saved(group) = web.create_wright_cause_group("wright tables").await? else {
        anyhow::bail!("the group was not created");
    };
    assert_eq!(
        web.create_wright_cause_group("wright tables").await?,
        GroupWrite::NameTaken
    );
    let GroupWrite::Saved(other) = web.create_wright_cause_group("steering").await? else {
        anyhow::bail!("the second group was not created");
    };
    assert_eq!(
        web.rename_wright_cause_group(other.id, "wright tables")
            .await?,
        GroupWrite::NameTaken
    );
    let into = CausePatch {
        group_id: Some(Some(group.id)),
        ..CausePatch::default()
    };
    env.patch(lone_cause, into).await?;
    env.patch(shared_cause, into).await?;
    let row = env.row(lone_cause).await?;
    assert_eq!(row.group_id, Some(group.id));
    assert_eq!(row.group_name.as_deref(), Some("wright tables"));
    let listed = env.persist.web().list_wright_cause_groups().await?;
    assert_eq!(
        listed
            .iter()
            .map(|g| (g.name.as_str(), g.causes))
            .collect::<Vec<_>>(),
        vec![("steering", 0), ("wright tables", 2)]
    );
    assert!(
        env.persist
            .web()
            .delete_wright_cause_group(group.id)
            .await?
    );
    assert!(
        !env.persist
            .web()
            .delete_wright_cause_group(group.id)
            .await?
    );
    let row = env.row(lone_cause).await?;
    assert_eq!((row.group_id, row.group_name), (None, None));
    env.patch(
        lone_cause,
        CausePatch {
            group_id: Some(None),
            ..CausePatch::default()
        },
    )
    .await?;
    env.done().await
}

#[tokio::test]
async fn the_list_says_why_the_engineer_will_not_take_a_pinned_cause() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let run = env.run(CHECK).await?;
    let (known, episode) = env.problem(CHECK, "query 1: SELECT known", run, 1).await?;
    let (fresh, _) = env.problem(CHECK, "query 2: SELECT fresh", run, 1).await?;
    env.persist.web().assign_wright_causes().await?;
    env.finding(known, episode).await?;
    let (known_cause, fresh_cause) = (env.cause(known).await?, env.cause(fresh).await?);
    env.patch(
        known_cause,
        CausePatch {
            pinned: Some(true),
            ..CausePatch::default()
        },
    )
    .await?;
    let row = env.row(known_cause).await?;
    assert!(row.pinned && !row.investigable);
    assert_eq!(row.not_investigated_because.as_deref(), Some("finding"));
    let pick = env.persist.web().pick_wright_problem(2).await?;
    assert_eq!(pick.map(|p| p.problem_id), Some(fresh));
    let row = env.row(fresh_cause).await?;
    assert!(row.investigable && row.not_investigated_because.is_none());
    env.done().await
}
