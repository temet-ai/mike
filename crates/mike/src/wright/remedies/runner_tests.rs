use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::web_repo::WebPersist as _;
use serde_json::json;

use super::fixtures_tests::{Env, owners_for};
use super::pass;

const TELL: &str = "tell_undelivered_senders";
const CHECK: &str = "undelivered_mail";

#[tokio::test]
async fn a_notice_is_sent_through_the_mailbox_owner_once_and_measured() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    env.only(TELL).await?;
    let stuck = env.stuck_mail(3).await?;
    assert_eq!(env.watch(CHECK).await?.opened, 1);

    let first = pass(&env.owners).await?;
    assert_eq!(first.verified, 1);
    assert_eq!(env.notices_to(stuck.sender).await?, 3);
    let actions = env.actions(TELL).await?;
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].state, "verified");
    assert_eq!(
        actions[0]
            .prior_state
            .as_ref()
            .map(|p| p["sender_notified"].clone()),
        Some(json!(false))
    );
    assert_eq!(
        actions[0].effect.as_ref().map(|e| e["outcome"].clone()),
        Some(json!("told_senders"))
    );
    assert_eq!(
        actions[0].effect.as_ref().map(|e| e["stamped"].clone()),
        Some(json!(3))
    );

    let second = pass(&env.owners).await?;
    assert_eq!((second.verified, second.refused), (0, 0));
    assert_eq!(env.actions(TELL).await?.len(), 1);
    assert_eq!(env.notices_to(stuck.sender).await?, 3);

    env.read_all(stuck.to).await?;
    assert_eq!(env.watch(CHECK).await?.healthy, 1);
    let measured = pass(&env.owners).await?.measured;
    assert_eq!(measured.len(), 1);
    assert_eq!((measured[0].effective, measured[0].turned_off), (1, false));
    assert_eq!(env.actions(TELL).await?[0].effective, Some(true));
    env.cleanup().await
}

#[tokio::test]
async fn competing_passes_act_on_one_episode_once() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    env.only(TELL).await?;
    let stuck = env.stuck_mail(3).await?;
    env.watch(CHECK).await?;
    let other = owners_for(&env.db.database_url).await?;
    let reports = futures::future::try_join_all((0..6).map(|i| {
        if i % 2 == 0 {
            pass(&env.owners)
        } else {
            pass(&other)
        }
    }))
    .await?;
    assert_eq!(reports.iter().map(|r| r.verified).sum::<usize>(), 1);
    assert_eq!(env.actions(TELL).await?.len(), 1);
    assert_eq!(env.notices_to(stuck.sender).await?, 3);
    env.cleanup().await
}

#[tokio::test]
async fn a_claim_left_by_a_crash_is_resumed_and_recorded_once() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    env.only(TELL).await?;
    let stuck = env.stuck_mail(3).await?;
    env.watch(CHECK).await?;
    let due = env.owners.persist.web().due_wright_actions(10).await?;
    let id = env
        .owners
        .persist
        .web()
        .claim_wright_action(TELL, due[0].problem_id, due[0].episode_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("claim refused"))?;

    let fresh = pass(&env.owners).await?;
    assert_eq!(
        fresh.verified, 0,
        "a fresh claim belongs to the worker that made it"
    );
    assert_eq!(env.notices_to(stuck.sender).await?, 0);

    env.backdate(id).await?;
    assert_eq!(pass(&env.owners).await?.verified, 1);
    assert_eq!(env.notices_to(stuck.sender).await?, 3);
    let actions = env.actions(TELL).await?;
    assert_eq!(
        (actions.len(), actions[0].id, actions[0].state.as_str()),
        (1, id, "verified")
    );
    env.cleanup().await
}

#[tokio::test]
async fn an_action_applied_before_a_crash_is_verified_by_reading_again_not_reapplied()
-> anyhow::Result<()> {
    let env = Env::boot().await?;
    env.only(TELL).await?;
    let stuck = env.stuck_mail(3).await?;
    env.watch(CHECK).await?;
    let web = env.owners.persist.web();
    let due = web.due_wright_actions(10).await?;
    let id = web
        .claim_wright_action(TELL, due[0].problem_id, due[0].episode_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("claim refused"))?;
    let notices = env
        .owners
        .persist
        .launch()
        .tell_senders_not_delivered(&stuck.messages)
        .await?;
    web.mark_wright_applied(
        id,
        &json!({
            "outcome": "told_senders",
            "recipient": stuck.recipient,
            "messages": stuck.messages,
            "notices": notices,
        }),
    )
    .await?;
    env.backdate(id).await?;
    assert_eq!(pass(&env.owners).await?.verified, 1);
    assert_eq!(env.notices_to(stuck.sender).await?, 3);
    let actions = env.actions(TELL).await?;
    assert_eq!(
        actions[0]
            .effect
            .as_ref()
            .map(|e| (e["notices"].clone(), e["stamped"].clone())),
        Some((json!(notices), json!(3)))
    );
    env.cleanup().await
}

#[tokio::test]
async fn the_daily_limit_stops_claims_for_the_rest_of_the_day() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    env.only(TELL).await?;
    env.limits(TELL, 1, 20).await?;
    env.stuck_mail(3).await?;
    env.stuck_mail(3).await?;
    assert_eq!(env.watch(CHECK).await?.opened, 2);
    assert_eq!(pass(&env.owners).await?.verified, 1);
    assert_eq!(pass(&env.owners).await?.verified, 0);
    assert_eq!(env.actions(TELL).await?.len(), 1);
    assert_eq!(env.remedy(TELL).await?.actions_today, 1);
    env.cleanup().await
}

#[tokio::test]
async fn three_refusals_in_a_row_pause_the_remedy_with_its_reason() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    env.only(TELL).await?;
    env.limits(TELL, 20, 2).await?;
    for _ in 0..4 {
        env.stuck_mail(3).await?;
    }
    assert_eq!(env.watch(CHECK).await?.opened, 4);
    let report = pass(&env.owners).await?;
    assert_eq!((report.refused, report.paused), (3, 1));
    let remedy = env.remedy(TELL).await?;
    assert!(remedy.paused_at.is_some());
    assert!(
        remedy
            .pause_reason
            .is_some_and(|r| r.contains("ceiling is 2"))
    );
    assert_eq!(env.actions(TELL).await?.len(), 3);
    assert_eq!(pass(&env.owners).await?.refused, 0);
    env.cleanup().await
}

#[tokio::test]
async fn wright_never_changes_its_own_settings() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    env.remedy_row(
        "own_setting",
        "setting",
        "set_setting",
        "request_failures",
        json!({"key": "wright.engineer_daily_runs", "value": 50}),
    )
    .await?;
    env.only("own_setting").await?;
    env.record("request_failures", &["anything".to_owned()])
        .await?;
    assert_eq!(pass(&env.owners).await?.refused, 1);
    let actions = env.actions("own_setting").await?;
    assert!(
        actions[0]
            .refusal
            .as_deref()
            .is_some_and(|r| r.contains("never changes its own"))
    );
    env.cleanup().await
}
