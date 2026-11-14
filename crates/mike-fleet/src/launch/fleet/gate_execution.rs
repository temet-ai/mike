use chrono::Utc;
use tokio::sync::watch;

use super::{
    InFlight, Target, admission, completion, in_flight, ledger_for, tell_waiting, waiting,
};
use crate::launch::fleet::check::Runner;
use crate::launch::fleet::definition::FleetDefinition;
use crate::launch::fleet::gate_queue::{after_run, completion_log};
use crate::launch::fleet::gate_slot::isolated;
use crate::launch::fleet::owned_spawn_fence::{Allowed, Authority, Identity};
use crate::store::persistence::fleet::TreeCheckRow;

pub async fn start(
    runner: &Runner<'_>,
    def: &FleetDefinition,
    seat: &str,
    target: &Target,
) -> Result<watch::Receiver<Option<TreeCheckRow>>, String> {
    let ledger = ledger_for(def, runner.run);
    let managed =
        crate::launch::fleet::managed_command::begin_gate(runner, def, seat, target).await?;
    let id = managed.id;
    let (sender, receiver) = watch::channel(None);
    let slot = register_in_flight(def, target, &ledger, &receiver);
    let (persist, run, def, seat, target, tracker) = (
        runner.persist.clone(),
        runner.run.to_string(),
        def.clone(),
        seat.to_string(),
        target.clone(),
        runner.tracker.clone(),
    );
    let context = managed.context;
    let registration = managed.registration;
    let owner = context.owner.clone();
    runner
        .tracker
        .spawn(crate::launch::fleet::managed_command::scope(
            context,
            async move {
                crate::launch::fleet::check_run::events::queued(&persist, &run, id, "gate").await;
                let log = completion_log(&target, &def.gate.command, &id.to_string());
                let (mut code, mut output, mut verified) =
                    execute_logged(&def, &target, &run, Some(log.clone())).await;
                let cancelled = crate::launch::fleet::managed_command::normalize_cancellation(
                    &owner,
                    &mut code,
                    &mut output,
                );
                if cancelled {
                    verified = false;
                }
                owner.phase(
                    crate::launch::fleet::owned_execution::Phase::RecordingReceipt,
                    None,
                );
                let captured = completion::Captured {
                    id,
                    exit_code: code,
                    output,
                    state_after: if cancelled {
                        "cancelled:unverified".into()
                    } else if verified {
                        target.commit.clone()
                    } else {
                        "unverified-source".into()
                    },
                };
                let row = match completion::record(&persist, &tracker, &ledger, &def.repo, captured)
                    .await
                {
                    Ok(row) => row,
                    Err(why) => {
                        registration.receipt_blocked(why);
                        return;
                    }
                };
                let finish = crate::launch::fleet::check_run::events::Finish {
                    receipt: id,
                    sha: &target.commit,
                    state: crate::launch::fleet::check_run::events::state_of(&row),
                    exit: row.exit_code,
                };
                crate::launch::fleet::check_run::events::finished(&persist, &run, finish).await;
                let sessions = {
                    let _held = admission::hold(&target.slot).await;
                    sender.send_replace(Some(row.clone()));
                    waiting()
                        .remove(&slot)
                        .map(|(_, sessions)| sessions)
                        .unwrap_or_default()
                };
                tell_waiting(&persist, (&def, &seat), &target, &row, sessions).await;
                if !cancelled {
                    after_run(
                        &persist,
                        &def,
                        &run,
                        &target,
                        (code, &row.output, &log, verified),
                    )
                    .await;
                }
                let _held = admission::hold(&target.slot).await;
                in_flight().remove(&slot);
                registration.receipt_recorded();
            },
        ));
    Ok(receiver)
}

fn register_in_flight(
    def: &FleetDefinition,
    target: &Target,
    ledger: &str,
    receiver: &watch::Receiver<Option<TreeCheckRow>>,
) -> String {
    let slot = admission::key(&target.slot).to_string_lossy().into_owned();
    in_flight().insert(
        slot.clone(),
        InFlight {
            ledger: ledger.to_string(),
            target: target.clone(),
            command: def.gate.command.clone(),
            started_at: Utc::now(),
            done: receiver.clone(),
        },
    );
    slot
}

pub async fn execute_verified(
    def: &FleetDefinition,
    target: &Target,
    run: &str,
) -> (Option<i32>, String, bool) {
    execute_logged(def, target, run, None).await
}

async fn newer_state(def: &FleetDefinition, target: &Target) -> Option<String> {
    let newest = crate::launch::fleet::gate_queue::commit_to_check(def).await?;
    if newest.commit == target.commit {
        return None;
    }
    let contains = [
        "merge-base",
        "--is-ancestor",
        &target.commit,
        &newest.commit,
    ];
    crate::launch::fleet::watch::git(&def.repo, &contains, None)
        .await
        .map(|_| newest.commit)
}

fn startup_baseline(authority: &Authority) -> bool {
    matches!(
        authority,
        Authority::Durable(Identity {
            allowed: Allowed::Starting,
            ..
        })
    )
}

async fn repointed(def: &FleetDefinition, target: &Target) -> Option<(Option<i32>, String, bool)> {
    let context = crate::launch::fleet::managed_command::current().ok()?;
    if startup_baseline(&context.authority) {
        return None;
    }
    let newer = newer_state(def, target).await?;
    context.owner.cancel(format!(
        "re-pointed: commit {newer} is now the newest state and contains {}; the gate runs there next instead of at the stale commit",
        target.commit
    ));
    Some((
        None,
        crate::launch::fleet::managed_command::cancelled(&context.owner),
        false,
    ))
}

async fn execute_logged(
    def: &FleetDefinition,
    target: &Target,
    run: &str,
    completion_log: Option<std::path::PathBuf>,
) -> (Option<i32>, String, bool) {
    let checkout = crate::launch::fleet::gate_slot::slot_for(&def.repo)
        .unwrap_or_else(|| std::path::PathBuf::from(&def.repo));
    let _held = match crate::launch::fleet::managed_command::hold_checkout(
        &checkout,
        Some(std::time::Duration::from_secs(7200)),
    )
    .await
    {
        Ok(held) => held,
        Err(why) => return (None, why, false),
    };
    if let Some(cancelled) = repointed(def, target).await {
        return cancelled;
    }
    isolated::execute(
        &def.repo,
        target,
        run,
        &def.gate.command,
        isolated::Operation::Gate { completion_log },
    )
    .await
}
