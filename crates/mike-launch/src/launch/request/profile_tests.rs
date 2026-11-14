use std::collections::BTreeMap;

use super::{seat_env, split_launch_args};
use crate::launch::seat::PROFILE_ENV;

fn environment(args: &[&str]) -> anyhow::Result<BTreeMap<String, String>> {
    let split = split_launch_args(args.iter().map(|arg| arg.to_string()))?;
    let mut env = BTreeMap::new();
    seat_env(&mut env, None, &split);
    Ok(env)
}

#[test]
fn manual_fleet_successor_gets_the_declared_role_profile() -> anyhow::Result<()> {
    let env = environment(&[
        "--run",
        "fleet-ink-123",
        "--seat",
        "acceptance-2",
        "--role",
        "acceptance",
    ])?;
    assert_eq!(env.get(PROFILE_ENV).map(String::as_str), Some("acceptance"));
    Ok(())
}

#[test]
fn fleet_seat_name_supplies_profile_when_role_is_absent() -> anyhow::Result<()> {
    for (seat, expected) in [("coordinator-2", "coordinator"), ("rust-T1", "worker")] {
        let env = environment(&["--run", "fleet-ink-123", "--seat", seat])?;
        assert_eq!(env.get(PROFILE_ENV).map(String::as_str), Some(expected));
    }
    Ok(())
}

#[test]
fn ordinary_launches_do_not_get_a_fleet_profile() -> anyhow::Result<()> {
    for args in [
        vec![],
        vec!["--role", "acceptance"],
        vec!["--run", "ordinary", "--seat", "acceptance"],
    ] {
        assert!(!environment(&args)?.contains_key(PROFILE_ENV));
    }
    Ok(())
}

#[test]
fn explicit_role_wins_over_a_misleading_seat_name() -> anyhow::Result<()> {
    let env = environment(&[
        "--run",
        "fleet-ink-123",
        "--seat",
        "acceptance-2",
        "--role",
        "worker",
    ])?;
    assert_eq!(env.get(PROFILE_ENV).map(String::as_str), Some("worker"));
    Ok(())
}
