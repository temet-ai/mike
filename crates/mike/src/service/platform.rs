#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Platform {
    Launchd,
    Systemd,
}

impl Platform {
    pub(crate) fn for_os(os: &str) -> Option<Self> {
        match os {
            "macos" => Some(Self::Launchd),
            "linux" => Some(Self::Systemd),
            _ => None,
        }
    }

    pub(crate) fn current() -> Option<Self> {
        Self::for_os(std::env::consts::OS)
    }

    pub(crate) fn manager_name(self) -> &'static str {
        match self {
            Self::Launchd => "launchd",
            Self::Systemd => "systemd",
        }
    }
}
