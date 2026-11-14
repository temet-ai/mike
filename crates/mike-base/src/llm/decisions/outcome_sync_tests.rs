use serde_json::json;

use super::outcome_sync_support_tests::{Env, NewRecord, pair};

#[tokio::test]
async fn request_linked_points_take_the_outcome_of_the_turn_holding_the_request()
-> anyhow::Result<()> {
    let Some(env) = Env::boot(2).await? else {
        return Ok(());
    };
    let turns = env.turns().await?;
    env.set_outcome(turns[0].id, "accepted", "operator").await?;
    env.set_outcome(turns[1].id, "corrected", "judge").await?;
    let mut accepted = NewRecord::new("text_origin", "chosen");
    accepted.session_id = Some(env.session_id);
    accepted.request_id = Some(turns[0].start_request_id);
    let mut corrected = NewRecord::new("diagnostic_phrase", "skipped");
    corrected.request_id = Some(turns[1].start_request_id);
    let (a, c) = (env.record(accepted).await?, env.record(corrected).await?);
    assert_eq!(env.sync().await?, 2);
    assert_eq!(
        env.labels(a).await?,
        vec![pair("right", "next_turn_reaction")]
    );
    assert_eq!(
        env.labels(c).await?,
        vec![pair("wrong", "next_turn_reaction")]
    );
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}

#[tokio::test]
async fn directive_with_nothing_applied_or_no_real_turn_label_gets_no_label() -> anyhow::Result<()>
{
    let Some(env) = Env::boot(3).await? else {
        return Ok(());
    };
    let turns = env.turns().await?;
    env.set_outcome(turns[0].id, "accepted", "operator").await?;
    env.set_outcome(turns[1].id, "accepted", "no_verdict")
        .await?;
    let mut unapplied = NewRecord::new("directive", "none");
    unapplied.operator_turn_id = Some(turns[0].id);
    let mut unsourced = NewRecord::new("directive", "chosen");
    unsourced.operator_turn_id = Some(turns[1].id);
    let mut unknown = NewRecord::new("directive", "chosen");
    unknown.operator_turn_id = Some(turns[2].id);
    let ids = [
        env.record(unapplied).await?,
        env.record(unsourced).await?,
        env.record(unknown).await?,
    ];
    assert_eq!(env.sync().await?, 0);
    for id in ids {
        assert!(env.labels(id).await?.is_empty());
    }
    env.finish().await
}

#[tokio::test]
async fn ended_on_by_operator_turn_id_is_labelled_from_the_turn_outcome() -> anyhow::Result<()> {
    let Some(env) = Env::boot(1).await? else {
        return Ok(());
    };
    let turns = env.turns().await?;
    env.set_outcome(turns[0].id, "shipped", "judge").await?;
    let mut record = NewRecord::new("ended_on", "chosen");
    record.operator_turn_id = Some(turns[0].id);
    let id = env.record(record).await?;
    assert_eq!(env.sync().await?, 1);
    assert_eq!(
        env.labels(id).await?,
        vec![pair("right", "next_turn_reaction")]
    );
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}

#[tokio::test]
async fn a_warning_note_followed_by_a_correction_is_right_and_by_acceptance_wrong()
-> anyhow::Result<()> {
    let Some(env) = Env::boot(3).await? else {
        return Ok(());
    };
    let turns = env.turns().await?;
    env.set_outcome(turns[0].id, "corrected", "operator")
        .await?;
    env.set_outcome(turns[1].id, "accepted", "operator").await?;
    env.set_outcome(turns[2].id, "corrected", "operator")
        .await?;
    let mut commit = NewRecord::new("commit_message", "warned");
    commit.operator_turn_id = Some(turns[0].id);
    let mut rule = NewRecord::new("rule_check", "warned");
    rule.operator_turn_id = Some(turns[1].id);
    let mut quiet = NewRecord::new("rule_check", "none");
    quiet.operator_turn_id = Some(turns[2].id);
    let (c, r, q) = (
        env.record(commit).await?,
        env.record(rule).await?,
        env.record(quiet).await?,
    );
    assert_eq!(env.sync().await?, 2);
    let source = "next_turn_reaction_after_note";
    assert_eq!(env.labels(c).await?, vec![pair("right", source)]);
    assert_eq!(env.labels(r).await?, vec![pair("wrong", source)]);
    assert!(env.labels(q).await?.is_empty());
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}

#[tokio::test]
async fn session_judge_outcome_is_scored_against_the_operators_turn_mix() -> anyhow::Result<()> {
    let Some(env) = Env::boot(3).await? else {
        return Ok(());
    };
    let turns = env.turns().await?;
    env.set_outcome(turns[0].id, "accepted", "operator").await?;
    env.set_outcome(turns[1].id, "accepted", "operator").await?;
    env.set_outcome(turns[2].id, "corrected", "operator")
        .await?;
    let mut ids = Vec::new();
    for outcome in ["success", "failure", "partial", "unknown"] {
        let mut record = NewRecord::new("session_judge", "sent")
            .verdict(json!({"score": 0.8, "outcome": outcome}));
        record.session_id = Some(env.session_id);
        ids.push(env.record(record).await?);
    }
    assert_eq!(env.sync().await?, 3);
    let source = "operator_turn_mix";
    assert_eq!(env.labels(ids[0]).await?, vec![pair("right", source)]);
    assert_eq!(env.labels(ids[1]).await?, vec![pair("wrong", source)]);
    assert_eq!(env.labels(ids[2]).await?, vec![pair("right", source)]);
    assert!(env.labels(ids[3]).await?.is_empty());
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}

#[tokio::test]
async fn session_judge_with_fewer_than_two_operator_labelled_turns_gets_no_label()
-> anyhow::Result<()> {
    let Some(env) = Env::boot(2).await? else {
        return Ok(());
    };
    let turns = env.turns().await?;
    env.set_outcome(turns[0].id, "accepted", "operator").await?;
    env.set_outcome(turns[1].id, "corrected", "judge").await?;
    let mut record = NewRecord::new("session_judge", "sent")
        .verdict(json!({"score": 0.8, "outcome": "success"}));
    record.session_id = Some(env.session_id);
    let id = env.record(record).await?;
    assert_eq!(env.sync().await?, 0);
    assert!(env.labels(id).await?.is_empty());
    env.finish().await
}
