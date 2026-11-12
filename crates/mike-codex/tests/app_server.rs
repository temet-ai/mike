mod support;

use std::time::Duration;

use mike_codex::{CodexError, CodexSeat};
use serde_json::json;

use support::Fixture;

const DEADLINE: Duration = Duration::from_secs(5);

#[test]
#[ignore = "subprocess fixture invoked only by the offline app-server tests"]
fn fake_app_server() -> anyhow::Result<()> {
    support::fake::run()
}

#[tokio::test]
async fn persistent_thread_completes_resumes_and_forks() -> anyhow::Result<()> {
    tokio::time::timeout(DEADLINE, async {
        let fixture = Fixture::new()?;
        let mut seat = CodexSeat::launch(fixture.options()).await?;
        let thread = seat.thread_id().to_owned();
        assert!(seat.model_is_listed("fixture-model").await?);
        assert!(!seat.model_is_listed("unknown-model").await?);
        let turn = seat.brief("finish").await?;
        let outcome = seat.wait_turn(&turn, DEADLINE).await?;
        assert_eq!(outcome.status, "completed");
        assert_eq!(outcome.final_text, "fixture complete");
        assert_eq!(outcome.commands, vec![("fixture-check".into(), Some(0))]);
        assert!(!outcome.observations_incomplete);
        seat.stop().await?;
        let resumed = CodexSeat::resume(fixture.options(), &thread).await?;
        assert_eq!(resumed.thread_id(), thread);
        resumed.stop().await?;
        let forked = CodexSeat::fork(fixture.options(), &thread).await?;
        assert_ne!(forked.thread_id(), thread);
        assert_eq!(forked.forked_from(), Some(thread.as_str()));
        forked.stop().await?;
        anyhow::Ok(())
    })
    .await??;
    Ok(())
}

#[tokio::test]
async fn active_turn_interrupt_reports_interrupted_completion() -> anyhow::Result<()> {
    tokio::time::timeout(DEADLINE, async {
        let fixture = Fixture::new()?;
        let mut seat = CodexSeat::launch(fixture.options()).await?;
        let turn = seat.brief("wait").await?;
        let event = seat
            .next_event()
            .await
            .ok_or_else(|| anyhow::anyhow!("missing turn start"))?;
        assert_eq!(event["method"], "turn/started");
        seat.interrupt(&turn).await?;
        assert_eq!(seat.wait_turn(&turn, DEADLINE).await?.status, "interrupted");
        seat.stop().await?;
        anyhow::Ok(())
    })
    .await??;
    Ok(())
}

#[tokio::test]
async fn rpc_refusal_and_unhandled_approval_are_explicit() -> anyhow::Result<()> {
    tokio::time::timeout(DEADLINE, async {
        let fixture = Fixture::new()?;
        let seat = CodexSeat::launch(fixture.options()).await?;
        let error = seat.call("fixture/refuse", json!({})).await;
        assert!(matches!(error, Err(CodexError::Rpc { method, .. }) if method == "fixture/refuse"));
        let reply = seat.call("fixture/approval", json!({})).await?;
        assert_eq!(reply["approvalError"]["code"], -32601);
        seat.stop().await?;
        anyhow::Ok(())
    })
    .await??;
    Ok(())
}

#[tokio::test]
async fn closed_process_and_missing_thread_shape_return_errors() -> anyhow::Result<()> {
    tokio::time::timeout(DEADLINE, async {
        let fixture = Fixture::new()?;
        let seat = CodexSeat::launch(fixture.options()).await?;
        let error = seat.call("fixture/close", json!({})).await;
        assert!(matches!(error, Err(CodexError::Closed("stdout"))));
        let mut options = fixture.options();
        options.developer_instructions = "missing thread id".into();
        let result = CodexSeat::launch(options).await;
        assert!(matches!(
            result,
            Err(CodexError::Shape {
                field: "thread.id",
                ..
            })
        ));
        anyhow::Ok(())
    })
    .await??;
    Ok(())
}

#[tokio::test]
async fn unavailable_executable_reports_spawn_error() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    let mut options = fixture.options();
    options.program = options.cwd.join("not-installed");
    assert!(matches!(
        CodexSeat::launch(options).await,
        Err(CodexError::Spawn { .. })
    ));
    Ok(())
}

#[tokio::test]
async fn dropped_client_terminates_app_server_process() -> anyhow::Result<()> {
    tokio::time::timeout(DEADLINE, async {
        let fixture = Fixture::new()?;
        let seat = CodexSeat::launch(fixture.options()).await?;
        let pid = seat
            .pid()
            .ok_or_else(|| anyhow::anyhow!("missing child pid"))?;
        drop(seat);
        loop {
            let status = tokio::process::Command::new("kill")
                .args(["-0", &pid.to_string()])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .await?;
            if !status.success() {
                break;
            }
            tokio::task::yield_now().await;
        }
        anyhow::Ok(())
    })
    .await??;
    Ok(())
}
