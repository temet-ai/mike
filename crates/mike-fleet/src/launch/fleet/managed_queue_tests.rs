use std::time::Duration;

use super::{
    Depth, Enlisted, Lane, admit, depth as depth_of, enlist, expected_wait, leave, name_obligation,
    patience_from, place, supersede, withdraw,
};
use crate::launch::fleet::owned_execution::registry;

#[tokio::test]
async fn only_the_asker_withdraws_a_receipt_and_its_owner_is_cancelled() -> anyhow::Result<()> {
    let run = format!("queue-withdraw-{}", uuid::Uuid::new_v4());
    let registration = registry()
        .fence(&run)
        .await
        .register(&run)
        .map_err(anyhow::Error::msg)?;
    let owner = registration.owner();
    let receipt = 9_000_001;
    let queued = Enlisted {
        repo: &run,
        lane: Lane::Gate,
        ledger: &run,
        seat: "seat-a",
        state: "c1",
    };
    enlist(receipt, &queued, owner.clone());
    assert_eq!(depth_of(&run, Lane::Gate).waiting, 1);
    assert!(withdraw(receipt, "seat-b").is_err_and(|why| why.contains("only its asker")));
    assert!(!owner.token().is_cancelled());
    let said = withdraw(receipt, "seat-a").map_err(anyhow::Error::msg)?;
    assert!(said.contains("was queued and is withdrawn"), "{said}");
    assert!(owner.token().is_cancelled());
    assert_eq!(depth_of(&run, Lane::Gate).waiting, 0);
    leave(Some(receipt));
    assert!(withdraw(receipt, "seat-a").is_err());
    registration.receipt_recorded();
    Ok(())
}

#[tokio::test]
async fn a_newer_request_for_the_same_obligation_supersedes_the_queued_older_one()
-> anyhow::Result<()> {
    let run = format!("queue-supersede-{}", uuid::Uuid::new_v4());
    let fence = registry().fence(&run).await;
    let old = fence.register("a").map_err(anyhow::Error::msg)?;
    let other = fence.register("b").map_err(anyhow::Error::msg)?;
    drop(fence);
    let (old_ledger, new_ledger, other_ledger) = (
        format!("{run}#ruling:n:old"),
        format!("{run}#ruling:n:new"),
        format!("{run}#ruling:m:x"),
    );
    for ledger in [&old_ledger, &new_ledger] {
        name_obligation(ledger, format!("{run}#obligation:o-1"));
    }
    name_obligation(&other_ledger, format!("{run}#obligation:o-2"));
    enlist(
        9_000_011,
        &Enlisted {
            repo: &run,
            lane: Lane::Check,
            ledger: &old_ledger,
            seat: "s",
            state: "c1",
        },
        old.owner(),
    );
    enlist(
        9_000_012,
        &Enlisted {
            repo: &run,
            lane: Lane::Check,
            ledger: &other_ledger,
            seat: "s",
            state: "c1",
        },
        other.owner(),
    );
    assert_eq!(supersede(&new_ledger, "c2"), vec![9_000_011]);
    assert!(old.owner().token().is_cancelled());
    assert!(!other.owner().token().is_cancelled());
    assert!(supersede(&old_ledger, "c1").is_empty());
    leave(Some(9_000_011));
    leave(Some(9_000_012));
    old.receipt_recorded();
    other.receipt_recorded();
    Ok(())
}

fn depth(waiting: usize, executing: usize, slots: usize) -> Depth {
    Depth {
        waiting,
        executing,
        slots,
    }
}

#[test]
fn a_free_slot_means_no_wait() {
    let measured = Duration::from_secs(600);
    assert_eq!(expected_wait(depth(0, 1, 2), measured), Duration::ZERO);
    assert_eq!(expected_wait(depth(0, 0, 1), measured), Duration::ZERO);
}

#[test]
fn each_full_round_ahead_costs_one_measured_run() {
    let measured = Duration::from_secs(600);
    assert_eq!(expected_wait(depth(2, 1, 1), measured), measured * 3);
    assert_eq!(expected_wait(depth(2, 2, 2), measured), measured * 2);
}

#[test]
fn a_waiter_whose_expected_wait_exceeds_its_patience_is_refused_with_the_numbers() {
    let measured = Some(Duration::from_secs(1800));
    let patience = Some(Duration::from_secs(1800));
    let why = admit(depth(3, 1, 1), measured, patience)
        .err()
        .unwrap_or_default();
    assert!(why.contains("queue is 4 deep"), "{why}");
    assert!(why.contains("expected wait 7200s"), "{why}");
    assert!(why.contains("patience 1800s"), "{why}");
    assert!(admit(depth(0, 1, 1), measured, patience).is_ok());
}

#[test]
fn with_nothing_measured_no_waiter_is_refused() {
    assert!(admit(depth(40, 1, 1), None, Some(Duration::from_secs(1))).is_ok());
}

#[test]
fn a_waiter_without_a_declared_timeout_is_never_refused_however_long_the_queue() {
    let measured = Some(Duration::from_secs(1800));
    assert!(admit(depth(40, 2, 2), measured, None).is_ok());
}

#[tokio::test]
async fn a_queued_receipt_reports_its_place_in_line_and_expected_wait() -> anyhow::Result<()> {
    let run = format!("queue-place-{}", uuid::Uuid::new_v4());
    let fence = registry().fence(&run).await;
    let regs = (0..3)
        .map(|n| {
            fence
                .register(&format!("{run}-{n}"))
                .map_err(anyhow::Error::msg)
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    drop(fence);
    let queued = Enlisted {
        repo: &run,
        lane: Lane::Gate,
        ledger: &run,
        seat: "s",
        state: "c1",
    };
    for (n, registration) in regs.iter().enumerate() {
        enlist(9_000_100 + n as i64, &queued, registration.owner());
    }
    let measured = Some(Duration::from_secs(300));
    let first = place(9_000_100, measured).ok_or_else(|| anyhow::anyhow!("not queued"))?;
    let last = place(9_000_102, measured).ok_or_else(|| anyhow::anyhow!("not queued"))?;
    assert_eq!((first.ahead_running, first.ahead_waiting), (0, 0));
    assert_eq!((last.ahead_running, last.ahead_waiting), (0, 2));
    assert_eq!(last.expected_wait, Some(Duration::from_secs(600)));
    assert!(
        last.describe().contains("place 3 in line"),
        "{}",
        last.describe()
    );
    assert!(last.describe().contains("expected wait about 600 s"));
    for n in 0..3 {
        leave(Some(9_000_100 + n));
    }
    for registration in regs {
        registration.receipt_recorded();
    }
    Ok(())
}

#[test]
fn default_patience_follows_the_measured_run_within_bounds() {
    assert_eq!(patience_from(None), Duration::from_secs(1800));
    assert_eq!(
        patience_from(Some(Duration::from_secs(1500))),
        Duration::from_secs(4500)
    );
    assert_eq!(
        patience_from(Some(Duration::from_secs(30))),
        Duration::from_secs(600)
    );
    assert_eq!(
        patience_from(Some(Duration::from_secs(5000))),
        Duration::from_secs(7200)
    );
}

fn build_operation() -> crate::launch::fleet::operation::Operation {
    crate::launch::fleet::operation::Operation {
        command: "CARGO_TARGET_DIR=target/fleet-check scripts/cx check -p mike --lib --tests"
            .into(),
        resources: vec!["mike-rust-build".into()],
        ..Default::default()
    }
}

#[tokio::test]
async fn a_commit_check_is_admitted_while_a_gate_holds_the_shared_build_and_a_floor_waits()
-> anyhow::Result<()> {
    use crate::launch::fleet::operation_admission::acquire;
    let root = tempfile::tempdir()?;
    let token = tokio_util::sync::CancellationToken::new();
    let gate = build_operation();
    let _gate_running = acquire(root.path(), &gate.resources, &token)
        .await
        .map_err(anyhow::Error::msg)?;

    let check =
        super::reserve_for_commit_check("fleet-open-issues-0c16da5b", Some(build_operation()))
            .ok_or_else(|| anyhow::anyhow!("operation dropped"))?;
    let admitted = tokio::time::timeout(
        Duration::from_secs(5),
        acquire(root.path(), &check.resources, &token),
    )
    .await;
    assert!(
        matches!(admitted, Ok(Ok(_))),
        "the commit check must not wait behind the gate"
    );

    let floor = super::reserve_for_commit_check(
        "fleet-open-issues-0c16da5b#ruling:floor-unit#baseline:abc",
        Some(build_operation()),
    )
    .ok_or_else(|| anyhow::anyhow!("operation dropped"))?;
    assert_eq!(floor.resources, build_operation().resources);
    let waited = tokio::time::timeout(
        Duration::from_millis(300),
        acquire(root.path(), &floor.resources, &token),
    )
    .await;
    assert!(
        waited.is_err(),
        "a floor keeps sharing the build resource with the gate"
    );
    Ok(())
}

#[test]
fn waiting_for_a_slot_or_resource_does_not_spend_the_run_budget() {
    assert_eq!(
        super::waiting_limit(Some(Duration::from_secs(600))),
        Some(super::QUEUE_WAIT)
    );
    assert!(super::QUEUE_WAIT >= Duration::from_secs(7200));
    assert_eq!(
        super::waiting_limit(Some(Duration::from_secs(9000))),
        Some(Duration::from_secs(9000))
    );
}

#[test]
fn without_a_declared_timeout_waiting_for_a_slot_has_no_limit() {
    assert_eq!(super::waiting_limit(None), None);
}
