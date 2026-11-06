use super::{Check, SetupStep};
use crate::service::{BackgroundFacts, Presence};

const ID: &str = "background_service";
const NAME: &str = "background service";

fn warn(detail: String) -> Check {
    Check::warn(ID, NAME, detail, Some(SetupStep::Service))
}

pub(super) fn background_service(facts: Option<&BackgroundFacts>) -> Option<Check> {
    let facts = facts?;
    let manager = facts.manager_name;
    if let Some(unit) = facts.units.iter().find(|u| u.presence == Presence::Foreign) {
        return Some(warn(format!(
            "{} exists but mike did not write it, so `mike service install` will not replace it",
            unit.file.display()
        )));
    }
    let installed: Vec<_> = facts
        .units
        .iter()
        .filter(|u| u.presence == Presence::Ours)
        .collect();
    if installed.is_empty() {
        return Some(warn(format!(
            "mike is not installed as a background service, so it runs only while a command started it. `mike service install` installs the proxy and the sidecar under {manager}"
        )));
    }
    let missing: Vec<&str> = facts
        .units
        .iter()
        .filter(|u| u.presence == Presence::Missing)
        .map(|u| u.kind.name())
        .collect();
    if !missing.is_empty() {
        return Some(warn(format!(
            "the {} service is not installed under {manager}; `mike service install` adds it",
            missing.join(" and ")
        )));
    }
    let stopped: Vec<String> = installed
        .iter()
        .filter(|u| {
            !u.activity.is_running() && !matches!(u.activity, crate::service::Activity::Unknown(_))
        })
        .map(|u| format!("{} is {}", u.kind.name(), u.activity.describe()))
        .collect();
    if !stopped.is_empty() {
        return Some(warn(format!(
            "installed under {manager}, but {}. `mike service status` shows why and `mike service restart` starts it again",
            stopped.join(" and ")
        )));
    }
    let unchecked = installed
        .iter()
        .find(|u| matches!(u.activity, crate::service::Activity::Unknown(_)));
    Some(match unchecked {
        Some(unit) => Check::ok(
            ID,
            NAME,
            format!(
                "installed under {manager}; whether it runs was not checked ({})",
                unit.activity.describe()
            ),
        ),
        None => Check::ok(
            ID,
            NAME,
            format!(
                "the proxy and the sidecar run under {manager} as {}",
                installed
                    .iter()
                    .map(|u| u.identifier)
                    .collect::<Vec<_>>()
                    .join(" and ")
            ),
        ),
    })
}
