#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]

use std::collections::VecDeque;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::Mutex;
use uuid::Uuid;

use super::{
    BackendId, Capabilities, Check, CloseOutcome, OpenRequest, ProbeReport, Surface,
    TerminalBackend, ValidationReport,
};
use crate::launch::error::LaunchError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FakeCall {
    Probe,
    Validate,
    Open(String),
    FindBySession(Uuid),
    Close(String),
    List,
}

#[derive(Default)]
pub(crate) struct FakeBackend {
    pub(crate) calls: Mutex<Vec<FakeCall>>,
    pub(crate) open_answers: Mutex<VecDeque<Result<Surface, LaunchError>>>,
    pub(crate) find_answers: Mutex<VecDeque<Result<Option<Surface>, LaunchError>>>,
    pub(crate) close_answers: Mutex<VecDeque<Result<CloseOutcome, LaunchError>>>,
    pub(crate) list_answers: Mutex<VecDeque<Result<Vec<Surface>, LaunchError>>>,
    pub(crate) open_requests: Mutex<Vec<OpenRequest>>,
    pub(crate) closed: Mutex<Vec<Surface>>,
    pub(crate) available: bool,
}

impl FakeBackend {
    pub(crate) fn available() -> Self {
        Self {
            available: true,
            ..Self::default()
        }
    }

    pub(crate) fn surface(session: Option<Uuid>) -> Surface {
        Surface {
            backend: BackendId::Herdr,
            workspace_ref: Some("w1".into()),
            tab_ref: Some("w1:t1".into()),
            surface_ref: "w1:p1".into(),
            pid: Some(4242),
            session_uuid: session,
            agent_state: Some("idle".into()),
            cwd: Some("/tmp/project".into()),
            actual_placement: "tab".into(),
            reused: false,
            delivered: None,
            confirmed: false,
            prompt_detail: None,
        }
    }

    fn opened(req: &OpenRequest) -> Surface {
        let given = req.prompt.is_some();
        Surface {
            delivered: given.then_some(true),
            confirmed: given,
            ..Self::surface(None)
        }
    }

    async fn record(&self, call: FakeCall) {
        self.calls.lock().await.push(call);
    }
}

#[async_trait]
impl TerminalBackend for FakeBackend {
    fn id(&self) -> BackendId {
        BackendId::Herdr
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            placement_workspace: true,
            placement_tab: true,
            placement_split: true,
            agent_state: true,
            prompt_in_argv: false,
            close_by_session: true,
            events: false,
        }
    }

    async fn probe(&self) -> ProbeReport {
        self.record(FakeCall::Probe).await;
        let check = if self.available {
            Check::pass("fake", "scripted available")
        } else {
            Check::fail("fake", "scripted unavailable", "flip the fake")
        };
        ProbeReport {
            backend: BackendId::Herdr,
            available: self.available,
            version: Some("0.0.0".into()),
            protocol: Some("22".into()),
            checks: vec![check],
        }
    }

    async fn validate(&self) -> ValidationReport {
        self.record(FakeCall::Validate).await;
        let check = if self.available {
            Check::pass("fake", "scripted pass")
        } else {
            Check::fail("fake", "scripted fail", "flip the fake")
        };
        ValidationReport::from_checks(BackendId::Herdr, vec![check], Utc::now())
    }

    async fn open(&self, req: &OpenRequest) -> Result<Surface, LaunchError> {
        self.record(FakeCall::Open(req.name.clone())).await;
        self.open_requests.lock().await.push(req.clone());
        self.open_answers
            .lock()
            .await
            .pop_front()
            .unwrap_or_else(|| Ok(Self::opened(req)))
    }

    async fn find_by_session(&self, session: Uuid) -> Result<Option<Surface>, LaunchError> {
        self.record(FakeCall::FindBySession(session)).await;
        self.find_answers
            .lock()
            .await
            .pop_front()
            .unwrap_or(Ok(None))
    }

    async fn close(&self, surface: &Surface, _wait: Duration) -> Result<CloseOutcome, LaunchError> {
        self.record(FakeCall::Close(surface.surface_ref.clone()))
            .await;
        self.closed.lock().await.push(surface.clone());
        self.close_answers
            .lock()
            .await
            .pop_front()
            .unwrap_or(Ok(CloseOutcome::NotFound))
    }

    async fn list(&self) -> Result<Vec<Surface>, LaunchError> {
        self.record(FakeCall::List).await;
        self.list_answers
            .lock()
            .await
            .pop_front()
            .unwrap_or(Ok(Vec::new()))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::launch::backend::Placement;

    #[tokio::test]
    async fn fake_records_calls_and_drains_queued_answers_in_order() {
        let fake = FakeBackend::available();
        let session = Uuid::new_v4();
        fake.find_answers
            .lock()
            .await
            .push_back(Ok(Some(FakeBackend::surface(Some(session)))));
        fake.close_answers
            .lock()
            .await
            .push_back(Err(LaunchError::Failed("scripted".into())));
        let req = OpenRequest {
            folder: "/tmp".into(),
            placement: Placement::Window,
            env: BTreeMap::new(),
            title: "t".into(),
            run_tab: None,
            run: None,
            program: "claude".into(),
            args: vec![],
            name: "cc-1".into(),
            prompt: None,
            prompt_wait_ms: 0,
        };
        let opened = fake.open(&req).await.unwrap();
        let found = fake.find_by_session(session).await.unwrap().unwrap();
        assert_eq!(found.session_uuid, Some(session));
        let err = fake
            .close(&opened, Duration::from_secs(1))
            .await
            .unwrap_err();
        assert_eq!(err.exit_code(), 1);
        assert_eq!(fake.list().await.unwrap(), Vec::<Surface>::new());
        assert!(fake.probe().await.available);
        assert!(fake.validate().await.passed);
        let calls = fake.calls.lock().await.clone();
        assert_eq!(
            calls,
            vec![
                FakeCall::Open("cc-1".into()),
                FakeCall::FindBySession(session),
                FakeCall::Close("w1:p1".into()),
                FakeCall::List,
                FakeCall::Probe,
                FakeCall::Validate,
            ]
        );
    }

    #[tokio::test]
    async fn fake_unavailable_reports_failed_checks() {
        let fake = FakeBackend::default();
        assert!(!fake.probe().await.available);
        assert!(!fake.validate().await.passed);
    }
}
