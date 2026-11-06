use super::endpoints::Endpoints;
use super::error::ServiceError;
use super::exec::Context;
use super::platform::Platform;
use super::spec::ServiceKind;
use super::state::Activity;
use crate::db_provision::{Call, Runner};

const DEV_STACK_PREFIX: &str = "mike-stack-";
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(300);

pub(crate) fn address_answers(addr: &str) -> bool {
    use std::net::{TcpStream, ToSocketAddrs};
    addr.to_socket_addrs()
        .ok()
        .and_then(|mut found| found.next())
        .is_some_and(|socket| TcpStream::connect_timeout(&socket, CONNECT_TIMEOUT).is_ok())
}

pub(crate) fn active_dev_units(listing: &str) -> Vec<String> {
    listing
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| name.starts_with(DEV_STACK_PREFIX))
        .map(ToString::to_string)
        .collect()
}

pub(crate) async fn refuse_dev_stack<R: Runner>(
    runner: &R,
    platform: Platform,
) -> Result<(), ServiceError> {
    if platform != Platform::Systemd {
        return Ok(());
    }
    let call = Call::new(
        "systemctl",
        &[
            "--user",
            "list-units",
            "mike-stack-*",
            "--state=active",
            "--no-legend",
            "--plain",
            "--no-pager",
        ],
    );
    let Ok(captured) = runner.run(&call).await else {
        return Ok(());
    };
    let units = active_dev_units(&captured.stdout);
    if !captured.success || units.is_empty() {
        return Ok(());
    }
    Err(ServiceError::DevStackActive {
        units: units.join(", "),
    })
}

pub(crate) fn refuse_taken_addresses(
    endpoints: &Endpoints,
    before: &[(ServiceKind, Activity)],
    listening: fn(&str) -> bool,
) -> Result<(), ServiceError> {
    for (kind, activity) in before {
        if activity.is_loaded() {
            continue;
        }
        for (what, addr) in endpoints.addresses(*kind) {
            if listening(&addr) {
                return Err(ServiceError::PortInUse { what, addr });
            }
        }
    }
    Ok(())
}

pub(crate) async fn refuse_conflicts<R: Runner>(
    ctx: &Context<'_, R>,
    before: &[(ServiceKind, Activity)],
) -> Result<(), ServiceError> {
    refuse_dev_stack(ctx.runner, ctx.manager.platform).await?;
    refuse_taken_addresses(ctx.endpoints, before, ctx.listening)
}
