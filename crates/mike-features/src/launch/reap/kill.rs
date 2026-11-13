use std::time::Duration;

use super::runner::ProcRunner;
use crate::launch::error::LaunchError;

async fn signal(runner: &dyn ProcRunner, pid: u32, args: &[&str]) {
    let mut argv: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    argv.push(pid.to_string());
    runner.run("kill", &argv).await;
}

async fn alive(runner: &dyn ProcRunner, pid: u32) -> bool {
    runner
        .run("kill", &["-0".to_string(), pid.to_string()])
        .await
        .ok()
}

async fn wait_gone(runner: &dyn ProcRunner, pid: u32, budget: Duration) -> bool {
    let step = (budget / 10).max(Duration::from_millis(10));
    let deadline = tokio::time::Instant::now() + budget;
    loop {
        if !alive(runner, pid).await {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(
            step.min(deadline.saturating_duration_since(tokio::time::Instant::now())),
        )
        .await;
    }
}

pub async fn kill(
    runner: &dyn ProcRunner,
    pid: u32,
    escalate_after: Duration,
) -> Result<(), LaunchError> {
    if !alive(runner, pid).await {
        return Ok(());
    }
    signal(runner, pid, &[]).await;
    if wait_gone(runner, pid, escalate_after).await {
        return Ok(());
    }
    signal(runner, pid, &["-9"]).await;
    if wait_gone(runner, pid, escalate_after).await {
        return Ok(());
    }
    Err(LaunchError::Failed(format!(
        "pid {pid} is still alive after SIGTERM and SIGKILL"
    )))
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
pub mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use async_trait::async_trait;

    use super::*;
    use crate::launch::reap::runner::ProcRunOutput;

    struct Scripted {
        alive_answers: Mutex<VecDeque<bool>>,
        calls: Mutex<Vec<Vec<String>>>,
    }

    fn out(ok: bool) -> ProcRunOutput {
        ProcRunOutput {
            stdout: String::new(),
            exit_code: i32::from(!ok),
            exec_error: None,
        }
    }

    #[async_trait]
    impl ProcRunner for Scripted {
        async fn run(&self, name: &str, args: &[String]) -> ProcRunOutput {
            self.calls.lock().unwrap().push(args.to_vec());
            if args.first().map(String::as_str) == Some("-0") {
                let alive = self
                    .alive_answers
                    .lock()
                    .unwrap()
                    .pop_front()
                    .unwrap_or(false);
                return out(alive);
            }
            assert_eq!(name, "kill");
            out(true)
        }
    }

    #[tokio::test]
    async fn kill_already_dead_pid_is_a_no_op() {
        let runner = Scripted {
            alive_answers: Mutex::new(VecDeque::from([false])),
            calls: Mutex::new(Vec::new()),
        };
        kill(&runner, 999, Duration::from_millis(50)).await.unwrap();
        assert_eq!(
            runner.calls.lock().unwrap().len(),
            1,
            "only the alive probe"
        );
    }

    #[tokio::test]
    async fn kill_sigterm_succeeds_without_escalating() {
        let runner = Scripted {
            alive_answers: Mutex::new(VecDeque::from([true, false])),
            calls: Mutex::new(Vec::new()),
        };
        kill(&runner, 1234, Duration::from_millis(50))
            .await
            .unwrap();
        let calls = runner.calls.lock().unwrap();
        assert!(!calls.iter().any(|c| c.iter().any(|a| a == "-9")));
    }

    #[tokio::test]
    async fn kill_escalates_to_sigkill_when_sigterm_does_not_land() {
        let runner = Scripted {
            alive_answers: Mutex::new(VecDeque::from(vec![true; 50])),
            calls: Mutex::new(Vec::new()),
        };
        let _ = kill(&runner, 1234, Duration::from_millis(20)).await;
        let calls = runner.calls.lock().unwrap();
        assert!(calls.iter().any(|c| c.iter().any(|a| a == "-9")));
    }

    #[tokio::test]
    async fn kill_still_alive_after_both_signals_is_an_error() {
        let runner = Scripted {
            alive_answers: Mutex::new(VecDeque::from([true; 20])),
            calls: Mutex::new(Vec::new()),
        };
        let err = kill(&runner, 1234, Duration::from_millis(10))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("still alive"));
    }
}
