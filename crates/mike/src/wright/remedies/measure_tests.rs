use serde_json::json;
use uuid::Uuid;

use super::fixtures_tests::Env;
use super::pass;

const DISABLE: &str = "disable_rules";
const CHECK: &str = "request_failures";

async fn three_rules(env: &Env) -> anyhow::Result<Vec<String>> {
    let mut rules = Vec::new();
    for value in ["wright-a", "wright-b", "wright-c"] {
        rules.push(env.filter_rule(value).await?.to_string());
    }
    env.remedy_row(DISABLE, "setting", "disable_filter_rule", CHECK, json!({}))
        .await?;
    env.only(DISABLE).await?;
    Ok(rules)
}

#[tokio::test]
async fn three_ineffective_runs_turn_the_fix_off_with_the_reason() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let rules = three_rules(&env).await?;
    env.record(CHECK, &rules).await?;
    assert_eq!(pass(&env.owners).await?.verified, 3);

    env.record(CHECK, &rules).await?;
    let measured = pass(&env.owners).await?.measured;
    assert_eq!(measured.len(), 1);
    assert_eq!((measured[0].ineffective, measured[0].turned_off), (3, true));
    let remedy = env.remedy(DISABLE).await?;
    assert!(!remedy.enabled);
    assert_eq!(remedy.ineffective_runs, 3);
    assert!(
        remedy
            .pause_reason
            .is_some_and(|r| r.starts_with("turned off"))
    );
    let states: Vec<String> = env
        .actions(DISABLE)
        .await?
        .into_iter()
        .map(|a| a.state)
        .collect();
    assert_eq!(states, ["ineffective", "ineffective", "ineffective"]);
    env.cleanup().await
}

#[tokio::test]
async fn an_unknown_run_does_not_count_as_a_pass_of_the_measure() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let rules = three_rules(&env).await?;
    env.record(CHECK, &rules[..1]).await?;
    pass(&env.owners).await?;
    env.unknown_run(CHECK).await?;
    assert!(pass(&env.owners).await?.measured.is_empty());
    env.cleanup().await
}

#[tokio::test]
async fn an_effective_run_resets_the_ineffective_count() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let rules = three_rules(&env).await?;
    env.ineffective_runs(DISABLE, 2).await?;
    env.record(CHECK, &rules[..1]).await?;
    pass(&env.owners).await?;
    env.record(CHECK, &[]).await?;
    let measured = pass(&env.owners).await?.measured;
    assert_eq!(
        (measured[0].effective, measured[0].ineffective_runs),
        (1, 0)
    );
    assert!(env.remedy(DISABLE).await?.enabled);
    env.cleanup().await
}

#[tokio::test]
async fn wright_own_sessions_are_left_out_of_actions_and_measures() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let rules = three_rules(&env).await?;
    env.record(CHECK, &rules[..1]).await?;
    assert_eq!(pass(&env.owners).await?.verified, 1);

    let own = Uuid::new_v4();
    env.own_session(own).await?;
    env.rename_resource(&rules[0], &own.to_string()).await?;
    let stranger = Uuid::new_v4();
    env.own_session(stranger).await?;
    env.record(CHECK, &[own.to_string(), stranger.to_string()])
        .await?;
    let report = pass(&env.owners).await?;
    assert!(report.measured.is_empty());
    assert_eq!(report.verified + report.refused, 0);
    assert_eq!(env.actions(DISABLE).await?.len(), 1);
    env.cleanup().await
}
