use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ServiceKind {
    Proxy,
    Sidecar,
}

impl ServiceKind {
    pub(crate) const ALL: [Self; 2] = [Self::Proxy, Self::Sidecar];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Proxy => "proxy",
            Self::Sidecar => "sidecar",
        }
    }

    pub(crate) fn description(self) -> &'static str {
        match self {
            Self::Proxy => "mike proxy, inspect API and web UI",
            Self::Sidecar => "mike forwarding sidecar",
        }
    }

    pub(crate) fn launchd_label(self) -> &'static str {
        match self {
            Self::Proxy => "ai.temet.mike.proxy",
            Self::Sidecar => "ai.temet.mike.forward",
        }
    }

    pub(crate) fn systemd_unit(self) -> &'static str {
        match self {
            Self::Proxy => "mike.service",
            Self::Sidecar => "mike-forward.service",
        }
    }

    pub(crate) fn arguments(self) -> &'static [&'static str] {
        match self {
            Self::Proxy => &["serve"],
            Self::Sidecar => &["forward", "serve"],
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Spec {
    pub(crate) kind: ServiceKind,
    pub(crate) exe: PathBuf,
    pub(crate) state_dir: PathBuf,
    pub(crate) environment: Vec<(String, String)>,
}

impl Spec {
    pub(crate) fn log_dir(&self) -> PathBuf {
        self.state_dir.join("logs")
    }

    pub(crate) fn log_path(&self) -> PathBuf {
        self.log_dir().join(format!("{}.log", self.kind.name()))
    }

    pub(crate) fn program_arguments(&self) -> Vec<String> {
        std::iter::once(self.exe.display().to_string())
            .chain(self.kind.arguments().iter().map(ToString::to_string))
            .collect()
    }
}
