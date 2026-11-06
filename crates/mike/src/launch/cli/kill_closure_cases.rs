use uuid::Uuid;

use crate::config::LaunchConfig;
use crate::launch::backend::fake::{FakeBackend, FakeCall};
use crate::launch::cli::ledger::Ledger;
use crate::launch::cli::queue::LocalQueue;
use crate::launch::cli::test_support::runner;
use crate::launch::{CloseOutcome, LaunchError};

pub(crate) async fn unconfirmed_kill_does_not_record_closure() -> anyhow::Result<()> {
    for outcome in [
        Ok(CloseOutcome::NotFound),
        Ok(CloseOutcome::Mismatch {
            detail: "surface identity changed during close".into(),
        }),
        Err(LaunchError::Failed("backend close timed out".into())),
    ] {
        let home = tempfile::tempdir()?;
        let session = Uuid::new_v4();
        let backend = FakeBackend::available();
        backend
            .find_answers
            .lock()
            .await
            .push_back(Ok(Some(FakeBackend::surface(Some(session)))));
        let expected_error = outcome.is_err();
        backend.close_answers.lock().await.push_back(outcome);
        let config = LaunchConfig::default();
        let mut runner = runner(&backend, &config, home.path())?;
        let queue = LocalQueue::at(home.path().join("queue"));
        runner.ledger = Ledger::none().with_queue(Some(queue.clone()));
        let result = super::execute(&runner, session).await;
        if expected_error {
            assert!(result.is_err());
        } else {
            let report = result?;
            assert!(!report.closed);
            assert_eq!(report.process_exited, None);
        }
        assert!(queue.closes().is_empty());
        assert!(!runner.ledger.lost_records());
        assert_eq!(
            *backend.calls.lock().await,
            vec![
                FakeCall::FindBySession(session),
                FakeCall::Close("w1:p1".into())
            ]
        );
    }
    Ok(())
}

pub(crate) async fn confirmed_pane_closure_preserves_exit_evidence() -> anyhow::Result<()> {
    for (pid, process_exited, expected) in [
        (Some(4242), false, Some(false)),
        (Some(4242), true, Some(true)),
        (None, false, None),
    ] {
        let home = tempfile::tempdir()?;
        let session = Uuid::new_v4();
        let backend = FakeBackend::available();
        let mut surface = FakeBackend::surface(Some(session));
        surface.pid = pid;
        backend
            .find_answers
            .lock()
            .await
            .push_back(Ok(Some(surface)));
        backend
            .close_answers
            .lock()
            .await
            .push_back(Ok(CloseOutcome::Closed { process_exited }));
        let config = LaunchConfig::default();
        let mut runner = runner(&backend, &config, home.path())?;
        let queue = LocalQueue::at(home.path().join("queue"));
        runner.ledger = Ledger::none().with_queue(Some(queue.clone()));
        let report = super::execute(&runner, session).await?;
        assert!(report.closed);
        assert_eq!(report.process_exited, expected);
        let recorded = queue.closes();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].1.session, session);
        assert_eq!(recorded[0].1.closed.reason, "kill --session");
        assert_eq!(recorded[0].1.closed.process_exited, expected);
        assert!(!runner.ledger.lost_records());
    }
    Ok(())
}
