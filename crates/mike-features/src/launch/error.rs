use std::time::Duration;

use super::backend::BackendId;

#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    #[error("usage: {0}")]
    Usage(String),
    #[error("refused: {0}")]
    Refused(String),
    #[error("{0}")]
    Failed(String),
    #[error("{what}: {source}")]
    Io {
        what: String,
        #[source]
        source: std::io::Error,
    },
    #[error("`{command}` did not answer within {after:?}")]
    Timeout { command: String, after: Duration },
    #[error("`{command}` failed: {code}: {message}")]
    BackendResponse {
        command: String,
        code: String,
        message: String,
    },
    #[error("`{command}` answered something unreadable: {detail}")]
    Malformed { command: String, detail: String },
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("unconfirmed: {0}")]
    Unconfirmed(String),
    #[error("backend {backend} unavailable: {detail}")]
    BackendUnavailable {
        backend: BackendId,
        detail: String,
        remedy: Option<String>,
    },
    #[error("no terminal backend is configured; choose a terminal backend in setup")]
    BackendNone,
    #[error("required executable not found: {name}")]
    ExecutableNotFound { name: String, remedy: String },
    #[error("{source}\npane output:\n{pane_tail}")]
    WithPaneOutput {
        #[source]
        source: Box<LaunchError>,
        pane_tail: String,
    },
}

impl LaunchError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Usage(_)
            | Self::Refused(_)
            | Self::Failed(_)
            | Self::Io { .. }
            | Self::Timeout { .. }
            | Self::BackendResponse { .. }
            | Self::Malformed { .. } => 1,
            Self::Unsupported(_) | Self::Unconfirmed(_) => 2,
            Self::BackendUnavailable { .. } | Self::BackendNone => 69,
            Self::ExecutableNotFound { .. } => 127,
            Self::WithPaneOutput { source, .. } => source.exit_code(),
        }
    }

    pub fn remedy(&self) -> Option<&str> {
        match self {
            Self::BackendUnavailable { remedy, .. } => remedy.as_deref(),
            Self::BackendNone => Some("choose a terminal backend in setup"),
            Self::ExecutableNotFound { remedy, .. } => Some(remedy),
            Self::WithPaneOutput { source, .. } => source.remedy(),
            _ => None,
        }
    }

    pub fn backend_code(&self) -> Option<&str> {
        match self {
            Self::BackendResponse { code, .. } => Some(code),
            Self::WithPaneOutput { source, .. } => source.backend_code(),
            _ => None,
        }
    }

    pub fn io(what: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            what: what.into(),
            source,
        }
    }

    pub fn with_pane_output(self, pane_tail: impl Into<String>) -> Self {
        let pane_tail = pane_tail.into();
        if pane_tail.trim().is_empty() {
            return self;
        }
        Self::WithPaneOutput {
            source: Box::new(self),
            pane_tail,
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
pub mod tests {
    use super::*;

    fn io_err() -> std::io::Error {
        std::io::Error::other("boom")
    }

    #[test]
    fn exit_code_each_variant_matches_contract_table() {
        let cases: Vec<(LaunchError, i32)> = vec![
            (LaunchError::Usage("u".into()), 1),
            (LaunchError::Refused("r".into()), 1),
            (LaunchError::Failed("f".into()), 1),
            (LaunchError::io("open", io_err()), 1),
            (
                LaunchError::Timeout {
                    command: "herdr api snapshot".into(),
                    after: Duration::from_secs(5),
                },
                1,
            ),
            (
                LaunchError::BackendResponse {
                    command: "herdr pane close".into(),
                    code: "pane_not_found".into(),
                    message: "pane w1:p1 not found".into(),
                },
                1,
            ),
            (
                LaunchError::Malformed {
                    command: "herdr agent list".into(),
                    detail: "no agents array".into(),
                },
                1,
            ),
            (LaunchError::Unsupported("--anchor-terminal".into()), 2),
            (LaunchError::Unconfirmed("prompt".into()), 2),
            (
                LaunchError::BackendUnavailable {
                    backend: BackendId::Herdr,
                    detail: "server down".into(),
                    remedy: Some("start herdr".into()),
                },
                69,
            ),
            (LaunchError::BackendNone, 69),
            (
                LaunchError::ExecutableNotFound {
                    name: "herdr".into(),
                    remedy: "install".into(),
                },
                127,
            ),
            (
                LaunchError::BackendResponse {
                    command: "herdr agent start".into(),
                    code: "timeout".into(),
                    message: "timed out waiting for agent startup".into(),
                }
                .with_pane_output("error: unknown option '--instructions'"),
                1,
            ),
        ];
        for (err, code) in cases {
            assert_eq!(err.exit_code(), code, "{err}");
        }
    }

    #[test]
    fn with_pane_output_empty_tail_is_a_no_op() {
        let err = LaunchError::Usage("u".into()).with_pane_output("   \n  ");
        assert!(matches!(err, LaunchError::Usage(_)), "{err}");
    }

    #[test]
    fn with_pane_output_wraps_and_delegates_backend_code_and_exit_code() {
        let err = LaunchError::BackendResponse {
            command: "herdr agent start".into(),
            code: "timeout".into(),
            message: "timed out waiting for agent startup".into(),
        }
        .with_pane_output("error: unknown option '--instructions'");
        assert_eq!(err.backend_code(), Some("timeout"));
        assert_eq!(err.exit_code(), 1);
        assert!(
            err.to_string()
                .contains("error: unknown option '--instructions'"),
            "{err}"
        );
    }

    #[test]
    fn remedy_backend_none_names_setup() {
        assert_eq!(
            LaunchError::BackendNone.remedy(),
            Some("choose a terminal backend in setup")
        );
        assert_eq!(LaunchError::Usage("x".into()).remedy(), None);
    }

    #[test]
    fn backend_code_only_for_backend_response() {
        let err = LaunchError::BackendResponse {
            command: "c".into(),
            code: "agent_not_ready".into(),
            message: "m".into(),
        };
        assert_eq!(err.backend_code(), Some("agent_not_ready"));
        assert_eq!(LaunchError::BackendNone.backend_code(), None);
    }
}
