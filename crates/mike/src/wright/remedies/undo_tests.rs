use crate::store::persistence::web_repo::WebPersist as _;
use serde_json::{Value, json};

use super::fixtures_tests::Env;
use super::pass;
use super::undo::{UndoError, can_undo, undo};
use crate::change_log::ChangeSource;
use crate::filter::service::FilterRulePatchInput;

const CHECK: &str = "request_failures";
const KEY: &str = "mailbox_stale_minutes";
const SET: &str = "stale_mail_window";

async fn setting(env: &Env) -> anyhow::Result<Option<Value>> {
    Ok(env
        .owners
        .persist
        .fetch_shared_settings()
        .await?
        .into_iter()
        .find_map(|(k, v)| (k == KEY).then_some(v)))
}

async fn operator_sets(env: &Env, value: i64) -> anyhow::Result<()> {
    let settings = env
        .owners
        .settings
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no settings service"))?;
    settings
        .set_for_machine_with_source(KEY, json!(value), None, &ChangeSource::internal())
        .await?;
    Ok(())
}

async fn wright_sets_77(env: &Env) -> anyhow::Result<i64> {
    env.remedy_row(
        SET,
        "setting",
        "set_setting",
        CHECK,
        json!({"key": KEY, "value": 77}),
    )
    .await?;
    env.only(SET).await?;
    env.record(CHECK, &["mail lag".to_owned()]).await?;
    assert_eq!(pass(&env.owners).await?.verified, 1);
    assert_eq!(setting(env).await?, Some(json!(77)));
    let actions = env.actions(SET).await?;
    assert!(can_undo(&actions[0]));
    Ok(actions[0].id)
}

#[tokio::test]
async fn a_setting_change_is_undone_through_the_settings_owner() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    operator_sets(&env, 30).await?;
    let id = wright_sets_77(&env).await?;
    let row = undo(&env.owners, id).await?;
    assert_eq!(row.state, "undone");
    assert!(!can_undo(&row));
    assert_eq!(setting(&env).await?, Some(json!(30)));
    assert!(row.effect.is_some_and(|e| e["undo"]["value"] == json!(30)));
    env.cleanup().await
}

#[tokio::test]
async fn an_undo_fails_cleanly_when_the_setting_changed_since() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let id = wright_sets_77(&env).await?;
    operator_sets(&env, 80).await?;
    let Err(UndoError::Conflict(reason)) = undo(&env.owners, id).await else {
        anyhow::bail!("an undo over a later change must conflict");
    };
    assert!(reason.contains("changed since"));
    assert_eq!(setting(&env).await?, Some(json!(80)));
    let row = env.actions(SET).await?.remove(0);
    assert_eq!(row.state, "undo_conflict");
    assert!(!can_undo(&row));
    env.cleanup().await
}

#[tokio::test]
async fn an_undo_sees_a_change_and_change_back_in_the_change_log() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let id = wright_sets_77(&env).await?;
    operator_sets(&env, 80).await?;
    operator_sets(&env, 77).await?;
    let Err(UndoError::Conflict(reason)) = undo(&env.owners, id).await else {
        anyhow::bail!("an undo over a later change must conflict");
    };
    assert!(reason.contains("change log"));
    env.cleanup().await
}

async fn disabled_rule(env: &Env) -> anyhow::Result<(i64, i64)> {
    let rule = env.filter_rule("wright-undo").await?;
    env.remedy_row(
        "disable_rule",
        "setting",
        "disable_filter_rule",
        CHECK,
        json!({}),
    )
    .await?;
    env.only("disable_rule").await?;
    env.record(CHECK, &[rule.to_string()]).await?;
    assert_eq!(pass(&env.owners).await?.verified, 1);
    Ok((rule, env.actions("disable_rule").await?[0].id))
}

async fn rule_enabled(env: &Env, id: i64) -> anyhow::Result<bool> {
    use crate::store::persistence::proxy_repo::ProxyPersist as _;
    Ok(env
        .owners
        .persist
        .proxy()
        .list_filter_rules()
        .await?
        .into_iter()
        .any(|rule| rule.id == id && rule.enabled))
}

#[tokio::test]
async fn a_disabled_filter_rule_is_enabled_again_by_its_undo() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let (rule, id) = disabled_rule(&env).await?;
    assert!(!rule_enabled(&env, rule).await?);
    assert_eq!(undo(&env.owners, id).await?.state, "undone");
    assert!(rule_enabled(&env, rule).await?);
    env.cleanup().await
}

#[tokio::test]
async fn a_filter_rule_edited_since_refuses_its_undo() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let (rule, id) = disabled_rule(&env).await?;
    let filters = env
        .owners
        .filters
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no filter service"))?;
    filters
        .update(
            rule,
            FilterRulePatchInput {
                value: Some("edited by the operator".to_owned()),
                ..FilterRulePatchInput::default()
            },
        )
        .await?;
    let Err(UndoError::Conflict(_)) = undo(&env.owners, id).await else {
        anyhow::bail!("an undo over a later edit must conflict");
    };
    assert!(!rule_enabled(&env, rule).await?);
    env.cleanup().await
}

#[tokio::test]
async fn a_sender_notice_has_no_undo_and_is_left_as_it_is() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    env.only("tell_undelivered_senders").await?;
    let stuck = env.stuck_mail(3).await?;
    env.watch("undelivered_mail").await?;
    pass(&env.owners).await?;
    let row = env.actions("tell_undelivered_senders").await?.remove(0);
    assert_eq!(row.state, "verified");
    assert!(!can_undo(&row));
    let Err(UndoError::Conflict(reason)) = undo(&env.owners, row.id).await else {
        anyhow::bail!("a sender notice has no undo");
    };
    assert!(reason.contains("cannot be undone"));
    let after = env
        .owners
        .persist
        .web()
        .wright_action(row.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("action vanished"))?;
    assert_eq!(after.state, "verified");
    assert_eq!(env.notices_to(stuck.sender).await?, 3);
    env.cleanup().await
}

#[tokio::test]
async fn an_unknown_action_is_not_found() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let Err(UndoError::NotFound(id)) = undo(&env.owners, 987_654).await else {
        anyhow::bail!("an unknown action must be not found");
    };
    assert_eq!(id, 987_654);
    env.cleanup().await
}
