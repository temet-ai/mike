use std::path::PathBuf;

use super::exec::{activity, run_loose};
use super::files::{self, Existing};
use super::manager::Manager;
use super::spec::ServiceKind;
use crate::db_provision::SystemRunner;

#[derive(Debug, Clone)]
pub(crate) struct InstalledProxy {
    pub(crate) manager_name: &'static str,
    pub(crate) identifier: &'static str,
    pub(crate) log: Option<PathBuf>,
}

pub(crate) fn installed_proxy() -> Option<InstalledProxy> {
    if crate::sandbox::active() {
        return None;
    }
    let manager = Manager::from_process().ok()?;
    let ours = matches!(
        files::inspect(&manager.file_path(ServiceKind::Proxy)),
        Ok(Existing::Ours(_))
    );
    ours.then(|| InstalledProxy {
        manager_name: manager.platform.manager_name(),
        identifier: manager.identifier(ServiceKind::Proxy),
        log: crate::user_config::state_dir()
            .ok()
            .map(|state| state.join("logs").join("proxy.log")),
    })
}

pub(crate) async fn start_proxy() -> bool {
    let Ok(manager) = Manager::from_process() else {
        return false;
    };
    let kind = ServiceKind::Proxy;
    let Ok(before) = activity(&SystemRunner, &manager, kind).await else {
        return false;
    };
    for call in manager.start_calls(kind, &before) {
        if !run_loose(&SystemRunner, &call)
            .await
            .is_ok_and(|captured| captured.success)
        {
            return false;
        }
    }
    true
}
