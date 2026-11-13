use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use super::{
    BackendId, Capabilities, Check, CloseOutcome, OpenRequest, ProbeReport, Surface,
    TerminalBackend, ValidationReport,
};
use crate::launch::error::LaunchError;

pub struct NoneBackend;

const HAND_LAUNCH: &str =
    "hand-launch only: `mike claude` in the current pane; `mike spawn open` is unavailable";

#[async_trait]
impl TerminalBackend for NoneBackend {
    fn id(&self) -> BackendId {
        BackendId::None
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            placement_workspace: false,
            placement_tab: false,
            placement_split: false,
            agent_state: false,
            prompt_in_argv: false,
            close_by_session: false,
            events: false,
        }
    }

    async fn probe(&self) -> ProbeReport {
        ProbeReport {
            backend: BackendId::None,
            available: true,
            version: None,
            protocol: None,
            checks: vec![Check::pass("terminal backend", HAND_LAUNCH)],
        }
    }

    async fn validate(&self) -> ValidationReport {
        ValidationReport::from_checks(
            BackendId::None,
            vec![Check::pass("terminal backend", HAND_LAUNCH)],
            Utc::now(),
        )
    }

    async fn open(&self, _req: &OpenRequest) -> Result<Surface, LaunchError> {
        Err(LaunchError::BackendNone)
    }

    async fn find_by_session(&self, _session: Uuid) -> Result<Option<Surface>, LaunchError> {
        Ok(None)
    }

    async fn close(
        &self,
        _surface: &Surface,
        _wait: Duration,
    ) -> Result<CloseOutcome, LaunchError> {
        Ok(CloseOutcome::NotFound)
    }

    async fn list(&self) -> Result<Vec<Surface>, LaunchError> {
        Ok(Vec::new())
    }
}

#[cfg(any(test, feature = "test-seams"))]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
pub mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::launch::backend::Placement;

    fn request() -> OpenRequest {
        OpenRequest {
            folder: "/tmp".into(),
            placement: Placement::Tab,
            env: BTreeMap::new(),
            title: "cc".into(),
            run_tab: None,
            run: None,
            program: "claude".into(),
            args: vec![],
            name: "cc-abcd1234".into(),
            prompt: None,
            prompt_wait_ms: 0,
        }
    }

    #[tokio::test]
    async fn open_refuses_with_backend_none_exit_69() {
        let err = NoneBackend.open(&request()).await.unwrap_err();
        assert!(matches!(err, LaunchError::BackendNone));
        assert_eq!(err.exit_code(), 69);
        assert_eq!(err.remedy(), Some("choose a terminal backend in setup"));
    }

    #[tokio::test]
    async fn reads_answer_nothing_and_probe_is_available() {
        assert!(NoneBackend.probe().await.available);
        assert!(NoneBackend.validate().await.passed);
        assert_eq!(
            NoneBackend.find_by_session(Uuid::new_v4()).await.unwrap(),
            None
        );
        assert!(NoneBackend.list().await.unwrap().is_empty());
        let surface = Surface {
            backend: BackendId::None,
            workspace_ref: None,
            tab_ref: None,
            surface_ref: "x".into(),
            pid: None,
            session_uuid: None,
            agent_state: None,
            cwd: None,
            actual_placement: "none".into(),
            reused: false,
            delivered: None,
            confirmed: false,
            prompt_detail: None,
        };
        assert_eq!(
            NoneBackend
                .close(&surface, Duration::from_millis(1))
                .await
                .unwrap(),
            CloseOutcome::NotFound
        );
        let caps = NoneBackend.capabilities();
        assert!(!caps.placement_tab && !caps.agent_state && !caps.close_by_session);
    }
}
