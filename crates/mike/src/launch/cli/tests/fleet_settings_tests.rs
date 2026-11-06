use std::io::Cursor;

use serde_json::{Value, json};

use super::open_tests::open_tokens;
use super::proxy_ledger_tests::{fake_proxy, hello_route, proxy_ledger, quiet_config};
use super::{fake, project_dir, runner, toks};
use crate::launch::cli::fleet_settings::inherit;
use crate::launch::cli::ledger::Ledger;
use crate::launch::cli::open;
use crate::launch::fleet::{FleetDefinition, seat_settings};
use crate::launch::{SplitLaunchArgs, split_launch_args};

const RUN: &str = "fleet-ink-migration-2-b870a477";
const FLEET_PATH: &str = "/api/fleets/ink-migration-2";

fn successor(extra: &[&str]) -> anyhow::Result<SplitLaunchArgs> {
    let mut args = toks(&["--run", RUN, "--seat", "acceptance-2"]);
    args.extend(toks(extra));
    Ok(split_launch_args(args)?)
}

fn settings(split: &SplitLaunchArgs) -> anyhow::Result<Value> {
    let raw = split
        .claude_args
        .windows(2)
        .find(|args| args[0] == "--settings")
        .ok_or_else(|| anyhow::anyhow!("no settings in {:?}", split.claude_args))?;
    Ok(serde_json::from_str(&raw[1])?)
}

#[tokio::test]
async fn successor_inherits_the_same_full_effective_settings_as_a_fleet_seat() -> anyhow::Result<()>
{
    let definition = json!({
        "repo": "/work/ink-migration",
        "execution": {"cargo": true},
        "seat_settings": {
            "env": {"EXTRA": "1", "TMPDIR": "/operator/tmp"},
            "enabledPlugins": {"other@plugins": false},
            "permissions": {"defaultMode": "acceptEdits"},
        },
    });
    let proxy = fake_proxy(vec![
        hello_route(),
        (
            "GET",
            FLEET_PATH.into(),
            200,
            json!({"run_id": RUN, "definition": definition}).to_string(),
        ),
    ])
    .await?;
    let ledger = proxy_ledger(&proxy.addr).await?;
    let def = FleetDefinition::from_value(&definition).map_err(anyhow::Error::msg)?;
    for (extra, kind) in [
        (vec!["--model", "opus"], "acceptance-2"),
        (
            vec!["--role", "coordinator", "--model", "opus"],
            "coordinator",
        ),
        (vec!["--role", "observer", "--model", "opus"], "observer"),
        (vec!["--role", "worker", "--model", "opus"], "worker"),
    ] {
        let mut split = successor(&extra)?;
        assert!(inherit(&ledger, &mut split).await?.is_some());
        let inherited = settings(&split)?;
        assert_eq!(inherited, seat_settings::for_seat(&def, kind));
        assert_eq!(inherited["env"]["TMPDIR"], "/operator/tmp");
        assert!(inherited["env"]["XDG_CONFIG_HOME"].as_str().is_some());
        assert!(inherited["env"]["CLAUDE_CODE_TMPDIR"].as_str().is_some());
        assert_eq!(
            inherited["enabledPlugins"]["rust-analyzer-lsp@claude-plugins-official"],
            false
        );
        assert_eq!(&split.claude_args[2..], &["--model", "opus"]);
        let inherited_split = split.clone();
        assert!(inherit(&ledger, &mut split).await?.is_none());
        assert_eq!(split, inherited_split);
    }
    let mut role_only = split_launch_args(toks(&["--run", RUN, "--role", "acceptance"]))?;
    inherit(&ledger, &mut role_only).await?;
    assert_eq!(
        settings(&role_only)?,
        seat_settings::for_seat(&def, "acceptance")
    );
    proxy.must_hit("GET", FLEET_PATH).await?;
    Ok(())
}

#[tokio::test]
async fn definition_profile_override_survives_successor_inheritance() -> anyhow::Result<()> {
    let proxy = fake_proxy(vec![
        hello_route(),
        (
            "GET",
            FLEET_PATH.into(),
            200,
            json!({
                "run_id": RUN,
                "definition": {"seat_settings": {"env": {"MIKE_MCP_PROFILE": "full"}}},
            })
            .to_string(),
        ),
    ])
    .await?;
    let ledger = proxy_ledger(&proxy.addr).await?;
    let mut split = successor(&["--role", "acceptance"])?;
    inherit(&ledger, &mut split).await?;
    assert_eq!(settings(&split)?["env"]["MIKE_MCP_PROFILE"], "full");
    Ok(())
}

#[tokio::test]
async fn explicit_caller_and_operator_settings_remain_complete_overrides() -> anyhow::Result<()> {
    for args in [
        vec!["--settings", r#"{"env":{"MIKE_MCP_PROFILE":"full"}}"#],
        vec!["--settings=/operator/settings.json"],
        vec!["--settings", "/operator/settings.json"],
    ] {
        let mut split = successor(&args)?;
        let before = split.clone();
        assert!(inherit(&Ledger::none(), &mut split).await?.is_none());
        assert_eq!(split, before);
        let mut split = open::with_operator_args(successor(&[])?, &toks(&args));
        let before = split.clone();
        assert!(inherit(&Ledger::none(), &mut split).await?.is_none());
        assert_eq!(split, before);
    }
    Ok(())
}

#[tokio::test]
async fn ordinary_and_retrospective_launches_do_not_read_or_change_fleet_settings()
-> anyhow::Result<()> {
    for args in [
        vec![],
        vec!["--role", "acceptance"],
        vec!["--run", "ordinary", "--seat", "acceptance-2"],
        vec!["--run", RUN],
        vec![
            "--run",
            "retro-ink-migration-2-b870a477",
            "--role",
            "retrospective",
        ],
    ] {
        let mut split = split_launch_args(toks(&args))?;
        let before = split.clone();
        assert!(inherit(&Ledger::none(), &mut split).await?.is_none());
        assert_eq!(split, before);
    }
    Ok(())
}

#[tokio::test]
async fn unknown_or_mismatched_fleet_runs_fail_without_changing_arguments() -> anyhow::Result<()> {
    for (status, body, expected) in [
        (404, "{}".to_string(), "no fleet is recorded"),
        (
            200,
            json!({"run_id": "fleet-ink-migration-2-deadbeef", "definition": {}}).to_string(),
            "no fleet is recorded",
        ),
        (
            200,
            json!({"run_id": null, "definition": {}}).to_string(),
            "no fleet is recorded",
        ),
        (
            200,
            json!({"run_id": RUN, "definition": {"repo": 7}}).to_string(),
            "invalid",
        ),
        (503, "unavailable".to_string(), "restore the fleet ledger"),
        (200, "{}".to_string(), "unreadable answer"),
    ] {
        let proxy = fake_proxy(vec![
            hello_route(),
            ("GET", FLEET_PATH.into(), status, body),
        ])
        .await?;
        let ledger = proxy_ledger(&proxy.addr).await?;
        let mut split = successor(&[])?;
        let before = split.clone();
        let error = inherit(&ledger, &mut split)
            .await
            .err()
            .ok_or_else(|| anyhow::anyhow!("invalid fleet must fail"))?;
        assert!(error.to_string().contains(expected), "{error}");
        assert_eq!(split, before);
    }
    Ok(())
}

#[tokio::test]
async fn invalid_run_ids_fail_before_any_ledger_lookup() -> anyhow::Result<()> {
    for run in [
        "fleet-ink",
        "fleet-ink-short",
        "fleet-ink-zzzzzzzz",
        "fleet-../ink-b870a477",
        "fleet-..-b870a477",
        "fleet--b870a477",
    ] {
        let mut split = split_launch_args(toks(&["--run", run, "--seat", "acceptance-2"]))?;
        let error = inherit(&Ledger::none(), &mut split)
            .await
            .err()
            .ok_or_else(|| anyhow::anyhow!("invalid run must fail"))?;
        assert!(
            error.to_string().contains("is not a fleet run id"),
            "{error}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn unavailable_fleet_settings_refuse_the_launch_before_the_backend_opens()
-> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let project = project_dir(home.path());
    let backend = fake();
    let config = quiet_config();
    let runner = runner(&backend, &config, home.path());
    let tokens = open_tokens(&project, &["--run", RUN, "--seat", "acceptance-2"]);
    let failure = open::execute(&runner, tokens, &mut Cursor::new(""))
        .await
        .err()
        .ok_or_else(|| anyhow::anyhow!("missing fleet settings must refuse the launch"))?;
    let refusal = failure.error.to_string();
    assert!(refusal.contains("needs a direct durable run fence"));
    assert!(refusal.contains("spawn_session"));
    assert_eq!(
        failure.output.and_then(|output| output.reason).as_deref(),
        Some("fleet_admission")
    );
    assert!(backend.open_requests.lock().await.is_empty());
    Ok(())
}
