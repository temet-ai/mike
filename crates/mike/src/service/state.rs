use crate::db_provision::Captured;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Activity {
    NotLoaded,
    Stopped(String),
    Running { pid: Option<u32> },
    Unknown(String),
}

impl Activity {
    pub(crate) fn is_loaded(&self) -> bool {
        matches!(self, Self::Stopped(_) | Self::Running { .. })
    }

    pub(crate) fn is_running(&self) -> bool {
        matches!(self, Self::Running { .. })
    }

    pub(crate) fn describe(&self) -> String {
        match self {
            Self::NotLoaded => "not loaded".to_string(),
            Self::Stopped(why) => format!("loaded but not running ({why})"),
            Self::Running { pid: Some(pid) } => format!("running (pid {pid})"),
            Self::Running { pid: None } => "running".to_string(),
            Self::Unknown(why) => format!("not checked ({why})"),
        }
    }
}

fn field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix(key))
        .map(str::trim)
}

fn pid_of(value: Option<&str>) -> Option<u32> {
    value
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|pid| *pid != 0)
}

pub(crate) fn from_launchctl_print(captured: &Captured) -> Activity {
    if !captured.success {
        return Activity::NotLoaded;
    }
    match field(&captured.stdout, "state =") {
        Some("running") => Activity::Running {
            pid: pid_of(field(&captured.stdout, "pid =")),
        },
        Some(other) => Activity::Stopped(other.to_string()),
        None => Activity::Stopped("no state reported".to_string()),
    }
}

fn systemd_value<'a>(text: &'a str, key: &str) -> &'a str {
    text.lines()
        .find_map(|line| line.strip_prefix(key)?.strip_prefix('='))
        .map_or("", str::trim)
}

pub(crate) fn from_systemctl_show(captured: &Captured) -> Activity {
    if !captured.success {
        return Activity::Unknown(captured.detail());
    }
    let text = &captured.stdout;
    if systemd_value(text, "LoadState") != "loaded" {
        return Activity::NotLoaded;
    }
    let active = systemd_value(text, "ActiveState");
    let sub = systemd_value(text, "SubState");
    if active == "active" && sub == "running" {
        return Activity::Running {
            pid: pid_of(Some(systemd_value(text, "MainPID"))),
        };
    }
    Activity::Stopped(format!("{active}, {sub}"))
}
