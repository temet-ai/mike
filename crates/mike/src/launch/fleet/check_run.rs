use crate::store::persistence::fleet_repo::FleetPersist as _;
use tokio::sync::watch;
use uuid::Uuid;

use super::check::{
    CheckRequest, Runner, Verdict, head, is_running, join_or_start, landed, state_for,
};
use super::check_proof;
use super::check_report::{definition_now, tell_owners, tell_waiting_with, verified};
use super::definition::FleetDefinition;
use super::gate::Target;
use super::gate_slot::{check_slot_for, isolated, prepare, run_check};
use super::seated::Seated;

#[path = "check_baseline.rs"]
pub(super) mod baseline;
#[path = "check_completion.rs"]
pub(super) mod completion;
#[path = "check_events.rs"]
pub(super) mod events;
use super::watch::git;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::TreeCheckRow;

const AUTOMATIC_SEAT: &str = "mike";
const FINISHED: &str = "finished";
const FOCUS_MARK: &str = "#test:";
const TEST_PLACEHOLDER: &str = "{test}";
const COMMAND_MARK: &str = "#command:";

pub(super) struct Focus {
    pub(super) ledger: String,
    pub(super) def: FleetDefinition,
}

pub(super) fn is_command(run: &str) -> bool {
    run.contains(COMMAND_MARK)
}

fn is_focused(run: &str) -> bool {
    run.contains(FOCUS_MARK) || run.contains(COMMAND_MARK)
}

pub(super) fn base_run(run: &str) -> &str {
    let run = baseline::base_run(run);
    let run = run.split_once(COMMAND_MARK).map_or(run, |(base, _)| base);
    let run = run.split_once(FOCUS_MARK).map_or(run, |(base, _)| base);
    run.split_once("#ruling:").map_or(run, |(base, _)| base)
}

fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

pub(super) fn focused_command(template: &str, test: &str) -> Result<String, String> {
    let test = test.trim();
    if test.is_empty() || test.chars().any(char::is_control) {
        return Err("`test` must name one test on one line".to_string());
    }
    if template.trim().is_empty() {
        return Err(
            "this fleet declares no focused-test command (definition `check.test_command`, with `{test}` where the test name goes)"
                .to_string(),
        );
    }
    let name = quoted(test);
    if template.contains(TEST_PLACEHOLDER) {
        Ok(template.replace(TEST_PLACEHOLDER, &name))
    } else {
        Ok(format!("{} {name}", template.trim_end()))
    }
}

pub(super) fn focus(def: &FleetDefinition, run: &str, test: &str) -> Result<Focus, String> {
    let command = focused_command(&def.check.test_command, test)?;
    let mut focused = def.clone();
    focused.check.command = command;
    Ok(Focus {
        ledger: format!("{run}{FOCUS_MARK}{}", test.trim()),
        def: focused,
    })
}

pub(super) fn requested(
    def: &FleetDefinition,
    run: &str,
    request: &CheckRequest,
) -> Result<Option<Focus>, String> {
    if let Some(problem) = super::operation::validate(def).first() {
        return Err(format!("{}: {}", problem.item, problem.problem));
    }
    if let Some(command) = &request.command {
        if request.test.is_some() || request.ruling.is_some() {
            return Err("choose a command, a focused test, or a ruling".into());
        }
        let command = command.trim();
        if command.is_empty() || command.contains('\0') {
            return Err("`command` must be nonempty and contain no NUL".into());
        }
        use sha2::{Digest, Sha256};
        let digest = hex::encode(Sha256::digest(command.as_bytes()));
        let mut focused = def.clone();
        focused.check.command = command.to_string();
        focused.check.paths.clear();
        return Ok(Some(Focus {
            ledger: format!("{run}{COMMAND_MARK}{digest}"),
            def: focused,
        }));
    }
    if !def.check.declared() {
        return Err(
            "this fleet declares no check command; give `command` or declare `check.command`"
                .into(),
        );
    }
    request
        .test
        .as_deref()
        .map(|test| focus(def, run, test))
        .transpose()
}

pub(super) async fn target(def: &FleetDefinition, commit: Option<&str>) -> Result<Target, String> {
    let Some(named) = commit.map(str::trim).filter(|c| !c.is_empty()) else {
        return head(def).await;
    };
    if named.starts_with('-') {
        return Err(format!("`{named}` is not a commit"));
    }
    let revision = format!("{named}^{{commit}}");
    let resolved = git(&def.repo, &["rev-parse", "--verify", "-q", &revision], None)
        .await
        .map(|raw| String::from_utf8_lossy(&raw).trim().to_string())
        .filter(|resolved| !resolved.is_empty())
        .ok_or_else(|| format!("`{named}` is not a commit of this repository"))?;
    let slot = check_slot_for(&def.repo)
        .ok_or_else(|| format!("`{}` is not a checkout for a baseline", def.repo))?;
    Ok(Target {
        commit: resolved,
        pushed: false,
        slot,
    })
}

pub(super) struct Started {
    pub(super) id: i64,
    pub(super) state: String,
    pub(super) at: Target,
    pub(super) managed: super::managed_command::Queued,
}

pub(super) fn start_run(
    runner: &Runner<'_>,
    def: FleetDefinition,
    started: Started,
    sender: watch::Sender<Option<TreeCheckRow>>,
) {
    let (persist, run, fleet) = (runner.persist.clone(), runner.run.to_string(), runner.fleet);
    let tracker = runner.tracker.clone();
    let context = started.managed.context;
    let registration = started.managed.registration;
    let owner = context.owner.clone();
    runner.tracker.spawn(super::managed_command::scope(
        context,
        Box::pin(async move {
            let id = started.id;
            events::queued(&persist, &run, id, "check").await;
            let (mut code, mut output) = Box::pin(execute(&def, &started.at, &run)).await;
            let cancelled =
                super::managed_command::normalize_cancellation(&owner, &mut code, &mut output);
            let state_after = if cancelled {
                "cancelled:unverified".into()
            } else {
                baseline::state_after(&run, &started.state, code)
            };
            owner.phase(super::owned_execution::Phase::RecordingReceipt, None);
            let captured = completion::Captured {
                id,
                exit_code: code,
                output,
                state_after,
            };
            let row = match Box::pin(completion::record(
                &persist, &tracker, &run, &def.repo, captured,
            ))
            .await
            {
                Ok(row) => row,
                Err(why) => {
                    registration.receipt_blocked(why);
                    return;
                }
            };
            let finish = events::Finish {
                receipt: id,
                sha: &started.at.commit,
                state: events::state_of(&row),
                exit: row.exit_code,
            };
            events::finished(&persist, &run, finish).await;
            let sessions = landed(&run, id).await;
            sender.send_replace(Some(row.clone()));
            let def = definition_now(&persist, fleet).await.unwrap_or(def);
            let verdict = Verdict {
                row,
                commit: started.at.commit,
                how: FINISHED.to_string(),
            };
            Box::pin(after(&persist, &def, &run, &verdict, sessions)).await;
            registration.receipt_recorded();
        }),
    ));
}

async fn after(
    persist: &PersistHandle,
    def: &FleetDefinition,
    run: &str,
    verdict: &Verdict,
    sessions: Vec<Uuid>,
) {
    if verified(verdict) && !baseline::is_baseline(run) && !is_command(run) {
        tell_owners(persist, def, verdict, None).await;
    }
    tell_waiting_with(persist, def, verdict, sessions).await;
    if verified(verdict) && !is_focused(run) && !baseline::is_baseline(run) {
        check_proof::record(persist, def, run, verdict).await;
    }
}

pub(super) fn checkout_used(repo: &str, isolated: bool) -> std::path::PathBuf {
    let clean = isolated
        .then(|| super::gate_slot::check_slot_for(repo))
        .flatten();
    clean.unwrap_or_else(|| std::path::PathBuf::from(repo))
}

pub(super) async fn execute(
    def: &FleetDefinition,
    at: &Target,
    run: &str,
) -> (Option<i32>, String) {
    let isolated = !is_command(run) || baseline::is_baseline(run);
    if !isolated {
        return check_shared_tree(def, base_run(run), &def.check.command).await;
    }
    let timeout = super::operation::timeout(def);
    let (code, output, _) = isolated::execute(
        &def.repo,
        at,
        base_run(run),
        &def.check.command,
        isolated::Operation::Check {
            timeout,
            configured: run == base_run(run),
        },
    )
    .await;
    (code, output)
}

pub(super) async fn check_shared_tree(
    def: &FleetDefinition,
    run: &str,
    command: &str,
) -> (Option<i32>, String) {
    let timeout = super::operation::timeout(def);
    let checkout = checkout_used(&def.repo, false);
    let _held = match super::managed_command::hold_checkout(&checkout, timeout).await {
        Ok(held) => held,
        Err(why) => return (None, why),
    };
    if let Err(why) = prepare(&def.repo).await {
        return (None, why);
    }
    run_check(&checkout, command, run, (timeout, "the check"), false).await
}

async fn reuse_recorded(
    runner: &Runner<'_>,
    def: &FleetDefinition,
    state: &str,
    commit: &str,
) -> bool {
    match runner
        .persist
        .fleet()
        .find_tree_check(runner.run, state)
        .await
    {
        Ok(Some(row)) => {
            let verdict = Verdict {
                row,
                commit: commit.to_string(),
                how: "reused".to_string(),
            };
            check_proof::record(runner.persist, def, runner.run, &verdict).await;
            true
        }
        Ok(None) => false,
        Err(_) => true,
    }
}

async fn head_needing_a_check(runner: &Runner<'_>, def: &FleetDefinition) -> Option<Target> {
    if !def.check.declared() || is_running(runner.run) {
        return None;
    }
    let at = head(def).await.ok()?;
    let seated = Seated::of_run(runner.persist, runner.run, def).await;
    check_proof::next_commit(def, &seated, runner.run, &at.commit).await?;
    Some(at)
}

pub(super) async fn queue(runner: &Runner<'_>, def: &FleetDefinition) {
    let Some(at) = head_needing_a_check(runner, def).await else {
        return;
    };
    let Ok(state) = state_for(def, &at.commit).await else {
        return;
    };
    if reuse_recorded(runner, def, &state, &at.commit).await {
        return;
    }
    if let Err(err) = join_or_start(runner, def, (AUTOMATIC_SEAT, &state, &at), None).await {
        tracing::warn!(run = runner.run, error = %err, "fleet: automatic check was not queued");
    }
}

#[cfg(test)]
#[path = "check_run_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "check_run_slot_tests.rs"]
mod slot_tests;
