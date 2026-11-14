use super::*;
use crate::launch::fleet::check::{CheckRequest, WAITING, check};
use crate::launch::fleet::check_run::requested;
use crate::launch::fleet::gate_slot::queue::hold;

fn command_request(command: &str) -> CheckRequest {
    CheckRequest {
        session: Uuid::from_u128(SEAT_SESSION),
        wait_seconds: None,
        status: false,
        test: None,
        command: Some(command.to_string()),
        commit: None,
        ruling: None,
        withdraw: None,
    }
}

#[test]
fn explicit_commands_need_no_declared_check_and_keep_their_own_receipts() -> anyhow::Result<()> {
    let def = parsed(&complete());
    let request = command_request("cargo nextest run -p gg --test mix");
    let focused = requested(&def, "fleet-command", &request)
        .map_err(anyhow::Error::msg)?
        .ok_or_else(|| anyhow::anyhow!("no command"))?;
    assert_eq!(
        focused.def.check.command,
        request.command.as_deref().unwrap_or_default()
    );
    assert!(focused.def.check.paths.is_empty());
    assert_eq!(base_run(&focused.ledger), "fleet-command");
    assert!(is_focused(&focused.ledger));
    assert!(!def.check.declared());
    for command in ["", "   ", "cargo check\0"] {
        assert!(requested(&def, "fleet-command", &command_request(command)).is_err());
    }
    for request in [
        CheckRequest {
            test: Some("one".into()),
            ..request.clone()
        },
        CheckRequest {
            ruling: Some(Uuid::nil()),
            ..request
        },
    ] {
        assert!(requested(&def, "fleet-command", &request).is_err());
    }
    Ok(())
}

#[tokio::test]
async fn one_checkout_queues_without_holding_the_caller_and_records_then_mails_real_results()
-> anyhow::Result<()> {
    managed::with_private_root(async {
        let Some(db) = acquire_ephemeral_db().await? else {
            eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
            return Ok(());
        };
        let persist = PersistHandle::connect(&db.database_url).await?;
        persist
            .launch()
            .ensure_session_exists(&Uuid::from_u128(SEAT_SESSION).to_string())
            .await?;
        let root = tempfile::tempdir()?;
        let (repo, def) = project(root.path()).await?;
        std::fs::create_dir(repo.join("target"))?;
        std::fs::write(repo.join("target/warm"), "cached\n")?;
        let held = hold(&repo).await;
        let tracker = TaskTracker::new();
        let run = "fleet-command-queue";
        let fleet = managed::running_fleet(&persist, &def, run).await?;
        seat_m(&persist, run).await?;
        let runner = || Runner {
            persist: &persist,
            tracker: &tracker,
            run,
            fleet,
        };
        let first = command_request("cat target/warm; printf first >> target/order; exit 7");
        let second = command_request("cat target/warm; printf second >> target/order");
        let mut receipts = Vec::new();
        for request in [&first, &second] {
            let receipt = tokio::time::timeout(
                std::time::Duration::from_secs(2),
                check(runner(), &def, &seated(), request),
            )
            .await?;
            assert_eq!(receipt.how, WAITING, "{receipt:?}");
            assert!(!receipt.passed && receipt.exit_code.is_none());
            let focused = requested(&def, run, request)
                .map_err(anyhow::Error::msg)?
                .ok_or_else(|| anyhow::anyhow!("no command"))?;
            let recorded = persist
                .fleet()
                .latest_tree_check(&focused.ledger)
                .await?
                .ok_or_else(|| anyhow::anyhow!("receipt returned before its record"))?;
            assert_eq!(receipt.receipt_id, Some(recorded.id));
            let status = check(
                runner(),
                &def,
                &seated(),
                &CheckRequest {
                    status: true,
                    ..request.clone()
                },
            )
            .await;
            assert_eq!(status.how, WAITING);
            assert_eq!(status.receipt_id, receipt.receipt_id);
            assert!(recorded.finished_at.is_none());
            assert_eq!(recorded.askers, vec!["db-dev-M"]);
            receipts.push(focused.ledger);
        }
        assert!(!repo.join("target/order").exists());
        drop(held);
        tracker.close();
        tracker.wait().await;
        for (ledger, code) in receipts.iter().zip([7, 0]) {
            let row = persist
                .fleet()
                .latest_tree_check(ledger)
                .await?
                .ok_or_else(|| anyhow::anyhow!("missing completed receipt"))?;
            assert_eq!(row.exit_code, Some(code));
            assert_eq!(row.output, "cached\n");
            assert!(row.finished_at.is_some());
        }
        assert_eq!(
            std::fs::read_to_string(repo.join("target/order"))?,
            "firstsecond"
        );
        assert_eq!(
            std::fs::read_to_string(repo.join("target/warm"))?,
            "cached\n"
        );
        assert!(!repo.join(".claude/worktrees/mike-check").exists());
        let mail = inbox(&persist).await?;
        assert!(
            mail.iter().any(|body| body.contains("exit 7` failed")),
            "{mail:?}"
        );
        assert!(
            mail.iter()
                .any(|body| body.contains("target/order` passed")),
            "{mail:?}"
        );
        db.cleanup().await?;
        Ok(())
    })
    .await?
}

#[tokio::test]
async fn separate_checkouts_keep_independent_slots_and_aliases_share_one() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let first = root.path().join("one");
    let second = root.path().join("two");
    std::fs::create_dir(&first)?;
    std::fs::create_dir(&second)?;
    let one = hold(&first).await;
    let two = tokio::time::timeout(std::time::Duration::from_secs(1), hold(&second)).await?;
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(10), hold(&first.join(".")))
            .await
            .is_err()
    );
    drop(one);
    let _again = tokio::time::timeout(std::time::Duration::from_secs(1), hold(&first)).await?;
    drop(two);
    Ok(())
}

#[tokio::test]
async fn a_failed_receipt_write_never_starts_the_command() -> anyhow::Result<()> {
    managed::with_private_root(async {
        let Some(db) = acquire_ephemeral_db().await? else {
            eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
            return Ok(());
        };
        let persist = PersistHandle::connect(&db.database_url).await?;
        let root = tempfile::tempdir()?;
        let (repo, def) = project(root.path()).await?;
        let fleet = managed::running_fleet(&persist, &def, "fleet-command-refused").await?;
        persist.pool().close().await;
        let tracker = TaskTracker::new();
        let runner = Runner {
            persist: &persist,
            tracker: &tracker,
            run: "fleet-command-refused",
            fleet,
        };
        let receipt = check(
            runner,
            &def,
            &seated(),
            &command_request("touch should-not-start"),
        )
        .await;
        assert_eq!(receipt.how, "refused");
        assert!(
            receipt
                .refused
                .is_some_and(|why| why.contains("not queued"))
        );
        assert!(receipt.receipt_id.is_none());
        tracker.close();
        tracker.wait().await;
        assert!(!repo.join("should-not-start").exists());
        db.cleanup().await?;
        Ok(())
    })
    .await?
}

#[tokio::test]
async fn status_reports_an_interrupted_durable_receipt_instead_of_claiming_nothing_ran()
-> anyhow::Result<()> {
    managed::with_private_root(async {
        let Some(db) = acquire_ephemeral_db().await? else {
            eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
            return Ok(());
        };
        let persist = PersistHandle::connect(&db.database_url).await?;
        let root = tempfile::tempdir()?;
        let (_, def) = project(root.path()).await?;
        let request = CheckRequest {
            status: true,
            ..command_request("cargo check")
        };
        let run = "fleet-interrupted-command";
        let fleet = managed::running_fleet(&persist, &def, run).await?;
        let focused = requested(&def, run, &request)
            .map_err(anyhow::Error::msg)?
            .ok_or_else(|| anyhow::anyhow!("missing command"))?;
        let id = persist
            .fleet()
            .begin_tree_check(&focused.ledger, "state", "cargo check", "db-dev-M")
            .await?;
        let tracker = TaskTracker::new();
        let receipt = check(
            Runner {
                persist: &persist,
                tracker: &tracker,
                run,
                fleet,
            },
            &def,
            &seated(),
            &request,
        )
        .await;
        assert_eq!(receipt.how, "refused");
        assert!(receipt.refused.is_some_and(
            |why| why.contains(&format!("receipt {id}")) && why.contains("interrupted")
        ));
        tracker.close();
        tracker.wait().await;
        db.cleanup().await?;
        Ok(())
    })
    .await?
}
