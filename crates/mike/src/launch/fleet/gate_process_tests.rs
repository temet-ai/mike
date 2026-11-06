use super::*;
use crate::launch::fleet::managed_test_support as managed;

#[tokio::test]
async fn timeout_preserves_failure_lines_and_incremental_log() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("gate.log");
    let (code, output) = managed::with_context(
        "fleet-test",
        run(
            dir.path(),
            "printf 'FAIL test::earlier\\n'; sleep 30",
            "fleet-test",
            (Duration::from_millis(200), "the gate"),
            Some(&path),
            None,
        ),
    )
    .await?;
    assert_eq!(code, None);
    assert!(output.starts_with("FAIL test::earlier\n"), "{output}");
    assert!(output.contains("was stopped after"), "{output}");
    assert_eq!(std::fs::read_to_string(path)?, output);
    Ok(())
}

#[tokio::test]
async fn combines_stdout_stderr_and_keeps_nonzero_exit() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (code, output) = managed::with_context(
        "fleet-test",
        run(
            dir.path(),
            "printf 'out\\n'; printf 'err\\n' >&2; exit 7",
            "fleet-test",
            (Duration::from_secs(5), "the gate"),
            None,
            None,
        ),
    )
    .await?;
    assert_eq!(code, Some(7));
    assert_eq!(output, "out\nerr\n");
    Ok(())
}

#[tokio::test]
async fn drains_output_larger_than_pipe_capacity_without_truncation() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (code, output) = managed::with_context(
        "fleet-test",
        run(
            dir.path(),
            "head -c 1048576 /dev/zero | tr '\\000' x",
            "fleet-test",
            (Duration::from_secs(5), "the gate"),
            None,
            None,
        ),
    )
    .await?;
    assert_eq!(code, Some(0));
    assert_eq!(output.len(), 1048576);
    assert!(output.bytes().all(|byte| byte == b'x'));
    Ok(())
}

#[tokio::test]
async fn timeout_kills_descendants_instead_of_leaving_them_working() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (code, _) = managed::with_context(
        "fleet-test",
        run(
            dir.path(),
            "(sleep 1; touch orphan-survived) & wait",
            "fleet-test",
            (Duration::from_millis(200), "the gate"),
            None,
            None,
        ),
    )
    .await?;
    assert_eq!(code, None);
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert!(!dir.path().join("orphan-survived").exists());
    Ok(())
}

#[tokio::test]
async fn exiting_shell_does_not_hang_on_descendants_inheriting_stdout() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        managed::with_context(
            "fleet-test",
            run(
                dir.path(),
                "sleep 30 & printf 'done\\n'",
                "fleet-test",
                (Duration::from_secs(20), "the gate"),
                None,
                None,
            ),
        ),
    )
    .await??;
    assert_eq!(result, (Some(0), "done\n".to_owned()));
    Ok(())
}

#[tokio::test]
async fn log_is_visible_before_command_finishes_and_cancellation_keeps_it() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let slot = dir.path().to_path_buf();
    let path = slot.join("gate.log");
    let log_path = path.clone();
    let run_id = uuid::Uuid::new_v4().to_string();
    let observed_run = run_id.clone();
    let def = crate::launch::fleet::FleetDefinition::default();
    let coordinator = uuid::Uuid::new_v4();
    let execution = managed::Execution::new(&run_id).await?;
    let context = execution.context();
    let task = tokio::spawn(crate::launch::fleet::managed_command::scope(
        context,
        async move {
            run(
                &slot,
                "printf 'FAIL live::test\\n'; sleep 30",
                &run_id,
                (Duration::from_secs(20), "the gate"),
                Some(&log_path),
                Some(crate::launch::fleet::process_tripwire::Setting::Gate),
            )
            .await
        },
    ));
    let seen = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if std::fs::read_to_string(&path)
                .unwrap_or_default()
                .contains("FAIL live::test")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    let observed =
        crate::launch::fleet::process_tripwire::notices(&observed_run, &def, coordinator);
    task.abort();
    let _ = task.await;
    execution.finish().await?;
    seen?;
    assert_eq!(observed.len(), 1);
    assert!(
        crate::launch::fleet::process_tripwire::notices(&observed_run, &def, coordinator)
            .is_empty()
    );
    let output = std::fs::read_to_string(path)?;
    assert!(output.starts_with("FAIL live::test\n"), "{output}");
    assert!(output.contains("managed command cancelled"), "{output}");
    Ok(())
}
