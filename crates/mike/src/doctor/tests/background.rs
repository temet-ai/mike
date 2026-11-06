use std::path::PathBuf;

use super::{check, healthy};
use crate::doctor::checks::evaluate;
use crate::doctor::{SetupStep, Status};
use crate::service::spec::ServiceKind;
use crate::service::status::UnitFacts;
use crate::service::{Activity, BackgroundFacts, Presence};

fn unit(kind: ServiceKind, presence: Presence, activity: Activity) -> UnitFacts {
    UnitFacts {
        kind,
        identifier: match kind {
            ServiceKind::Proxy => "ai.temet.mike.proxy",
            ServiceKind::Sidecar => "ai.temet.mike.forward",
        },
        file: PathBuf::from(format!("/agents/{}.plist", kind.name())),
        presence,
        activity,
    }
}

fn background_check(units: Vec<UnitFacts>) -> anyhow::Result<crate::doctor::Check> {
    let mut observed = healthy();
    observed.background = Some(BackgroundFacts {
        manager_name: "launchd",
        units,
    });
    let checks = evaluate(&observed);
    Ok(check(&checks, "background_service")?.clone())
}

fn running(kind: ServiceKind) -> UnitFacts {
    unit(kind, Presence::Ours, Activity::Running { pid: Some(10) })
}

#[test]
fn no_check_is_made_on_a_system_without_a_service_manager() {
    let checks = evaluate(&healthy());
    assert!(checks.iter().all(|c| c.id != "background_service"));
}

#[test]
fn nothing_installed_warns_and_names_the_install_command() -> anyhow::Result<()> {
    let result = background_check(vec![
        unit(ServiceKind::Proxy, Presence::Missing, Activity::NotLoaded),
        unit(ServiceKind::Sidecar, Presence::Missing, Activity::NotLoaded),
    ])?;
    assert_eq!(result.status, Status::Warn);
    assert!(
        result.detail.contains("mike service install"),
        "{}",
        result.detail
    );
    assert_eq!(result.setup_step, Some(SetupStep::Service));
    Ok(())
}

#[test]
fn both_services_running_is_ok() -> anyhow::Result<()> {
    let result = background_check(vec![
        running(ServiceKind::Proxy),
        running(ServiceKind::Sidecar),
    ])?;
    assert_eq!(result.status, Status::Ok);
    assert!(
        result
            .detail
            .contains("ai.temet.mike.proxy and ai.temet.mike.forward"),
        "{}",
        result.detail
    );
    Ok(())
}

#[test]
fn a_missing_sidecar_service_warns() -> anyhow::Result<()> {
    let result = background_check(vec![
        running(ServiceKind::Proxy),
        unit(ServiceKind::Sidecar, Presence::Missing, Activity::NotLoaded),
    ])?;
    assert_eq!(result.status, Status::Warn);
    assert!(
        result
            .detail
            .contains("the sidecar service is not installed"),
        "{}",
        result.detail
    );
    Ok(())
}

#[test]
fn an_installed_service_that_is_not_running_warns_with_the_status_command() -> anyhow::Result<()> {
    let result = background_check(vec![
        unit(
            ServiceKind::Proxy,
            Presence::Ours,
            Activity::Stopped("spawn scheduled".to_string()),
        ),
        running(ServiceKind::Sidecar),
    ])?;
    assert_eq!(result.status, Status::Warn);
    assert!(
        result.detail.contains("proxy is loaded but not running"),
        "{}",
        result.detail
    );
    assert!(
        result.detail.contains("mike service status"),
        "{}",
        result.detail
    );
    Ok(())
}

#[test]
fn a_file_mike_did_not_write_warns_without_claiming_an_install() -> anyhow::Result<()> {
    let result = background_check(vec![
        unit(ServiceKind::Proxy, Presence::Foreign, Activity::NotLoaded),
        unit(ServiceKind::Sidecar, Presence::Missing, Activity::NotLoaded),
    ])?;
    assert_eq!(result.status, Status::Warn);
    assert!(
        result.detail.contains("mike did not write it"),
        "{}",
        result.detail
    );
    Ok(())
}

#[test]
fn services_whose_state_was_not_checked_are_ok_and_say_so() -> anyhow::Result<()> {
    let why = || Activity::Unknown("switched off".to_string());
    let result = background_check(vec![
        unit(ServiceKind::Proxy, Presence::Ours, why()),
        unit(ServiceKind::Sidecar, Presence::Ours, why()),
    ])?;
    assert_eq!(result.status, Status::Ok);
    assert!(
        result.detail.contains("was not checked"),
        "{}",
        result.detail
    );
    Ok(())
}
