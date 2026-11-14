use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::collections::BTreeMap;

use uuid::Uuid;

use super::check::{Asker, Blocking, CheckReport, Verdict, asker, found_in, short, short_commit};
use super::check_run::{base_run, baseline, is_command};
use super::commit::{covers, owner_of};
use super::definition::FleetDefinition;
use super::diagnostics::Diagnostic;
use super::seated::Seated;
use crate::store::persistence::PersistHandle;

const TAIL_LINES: usize = 40;

pub fn verified(verdict: &Verdict) -> bool {
    let row = &verdict.row;
    row.finished_at.is_some()
        && row.exit_code.is_some()
        && row.state_after.as_deref() == Some(row.state.as_str())
}

pub async fn report(
    persist: &PersistHandle,
    def: &FleetDefinition,
    who: &Asker,
    verdict: Verdict,
) -> CheckReport {
    if verified(&verdict)
        && !baseline::is_baseline(&verdict.row.run_id)
        && !is_command(&verdict.row.run_id)
    {
        tell_owners(persist, def, &verdict, Some(&who.seat)).await;
    }
    answer(def, who, &verdict)
}

pub fn answer(def: &FleetDefinition, who: &Asker, verdict: &Verdict) -> CheckReport {
    let row = &verdict.row;
    let found = baseline::diagnostics(def, row);
    let sorted = sort(def, who, &found);
    let ran_nothing =
        super::nextest_filter::matched_no_test(&row.command, row.exit_code, &row.output);
    let (how, output_tail) = if ran_nothing {
        let why = super::nextest_filter::definition_problem(
            "this command or the done_when line that names it",
        );
        (
            super::nextest_filter::RAN_NO_TEST.to_string(),
            format!("{why}\n{}", tail(&row.output)),
        )
    } else {
        (verdict.how.clone(), tail(&row.output))
    };
    CheckReport {
        receipt_id: Some(row.id),
        command_receipt: is_command(&row.run_id),
        baseline: baseline::is_baseline(&row.run_id),
        seat: who.seat.clone(),
        command: row.command.clone(),
        state: row.state.clone(),
        commit: verdict.commit.clone(),
        how,
        passed: row.passed(),
        exit_code: row.exit_code,
        seconds: row
            .finished_at
            .map(|done| (done - row.started_at).num_seconds())
            .unwrap_or_default(),
        askers: row.askers.clone(),
        mine: sorted.mine,
        others: sorted.others,
        unowned: sorted.unowned,
        owned: who.owned.clone(),
        output_tail,
        refused: None,
        started_at: Some(row.started_at),
        queue: None,
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Sorted {
    pub mine: Vec<Diagnostic>,
    pub others: Vec<Blocking>,
    pub unowned: Vec<Diagnostic>,
}

pub fn sort(def: &FleetDefinition, who: &Asker, found: &[Diagnostic]) -> Sorted {
    let mut sorted = Sorted::default();
    for d in found {
        if who.everything || covers(&who.owned, &d.path) {
            sorted.mine.push(d.clone());
            continue;
        }
        match owner_of(def, &d.path) {
            Some((task, seat)) => sorted.others.push(Blocking {
                path: d.path.clone(),
                line: d.line,
                text: d.text.clone(),
                seat: seat.to_string(),
                task: task.clone(),
            }),
            None => sorted.unowned.push(d.clone()),
        }
    }
    sorted
}

pub async fn tell_owners(
    persist: &PersistHandle,
    def: &FleetDefinition,
    verdict: &Verdict,
    except: Option<&str>,
) {
    let nobody = Asker {
        seat: String::new(),
        owned: Vec::new(),
        everything: false,
    };
    let found = baseline::diagnostics(def, &verdict.row);
    let others = sort(def, &nobody, &found).others;
    let by_task = group_by_task(&others, except);
    let sessions = task_sessions(persist, def, &verdict.row.run_id).await;
    for (task, lines) in by_task {
        if let Some(session) = sessions.get(task).copied() {
            tell_one(persist, verdict, session, &lines).await;
        }
    }
}

fn group_by_task<'a>(
    others: &'a [Blocking],
    except: Option<&str>,
) -> BTreeMap<&'a str, Vec<&'a Blocking>> {
    let mut by_task: BTreeMap<&str, Vec<&Blocking>> = BTreeMap::new();
    for b in others.iter().filter(|b| Some(b.seat.as_str()) != except) {
        by_task.entry(b.task.as_str()).or_default().push(b);
    }
    by_task
}

async fn tell_one(persist: &PersistHandle, verdict: &Verdict, session: Uuid, lines: &[&Blocking]) {
    let Some(seat) = lines.first().map(|b| b.seat.clone()) else {
        return;
    };
    if !matches!(
        persist
            .fleet()
            .add_tree_check_told(verdict.row.id, &seat)
            .await,
        Ok(true)
    ) {
        return;
    }
    let body = owner_mail(verdict, lines);
    if let Err(err) = persist
        .launch()
        .insert_message(Uuid::new_v4(), session, None, &body)
        .await
    {
        tracing::warn!(%seat, error = %err, "fleet: could not tell a seat its file fails the check");
    }
}

pub async fn task_sessions(
    persist: &PersistHandle,
    def: &FleetDefinition,
    run: &str,
) -> BTreeMap<String, Uuid> {
    super::definition::task_sessions(persist, run, def).await
}

fn owner_mail(verdict: &Verdict, lines: &[&Blocking]) -> String {
    let listed: Vec<String> = lines
        .iter()
        .map(|b| format!("- {}:{} {}", b.path, b.line, b.text))
        .collect();
    format!(
        "[mike check] `{}` fails at commit {} (state {}) in your committed files, and every seat reads this result. Fix these and commit; mike checks the new commit by itself:\n{}",
        verdict.row.command,
        short_commit(&verdict.commit),
        short(&verdict.row.state),
        listed.join("\n")
    )
}

fn tail(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    let start = lines.len().saturating_sub(TAIL_LINES);
    lines[start..].join("\n")
}

pub async fn definition_now(persist: &PersistHandle, fleet: Uuid) -> Option<FleetDefinition> {
    let found = persist
        .fleet()
        .find_fleet(&fleet.to_string())
        .await
        .ok()??;
    FleetDefinition::from_value(&found.definition).ok()
}

async fn mail_the_result(
    persist: &PersistHandle,
    (def, seated): (&FleetDefinition, &Seated),
    verdict: &Verdict,
    session: Uuid,
) {
    let Some(who) = asker(def, seated, session) else {
        return;
    };
    let answer = report(persist, def, &who, verdict.clone()).await;
    let body = format!("[mike check] {}", text(&answer));
    if let Err(err) = persist
        .launch()
        .insert_message(Uuid::new_v4(), session, None, &body)
        .await
    {
        tracing::warn!(seat = %who.seat, error = %err, "fleet: could not mail a finished check to a seat waiting for it");
    }
}

pub async fn tell_waiting_with(
    persist: &PersistHandle,
    def: &FleetDefinition,
    verdict: &Verdict,
    sessions: Vec<Uuid>,
) {
    let seated = Seated::of_run(persist, base_run(&verdict.row.run_id), def).await;
    for session in sessions {
        mail_the_result(persist, (def, &seated), verdict, session).await;
    }
    if !verified(verdict)
        || baseline::is_baseline(&verdict.row.run_id)
        || is_command(&verdict.row.run_id)
    {
        return;
    }
    record_finished(
        persist,
        def,
        Finished {
            label: "check",
            run: &verdict.row.run_id,
            command: &verdict.row.command,
            commit: &verdict.commit,
            passed: verdict.row.passed(),
            source_verified: true,
            output: &verdict.row.output,
            root: baseline::diagnostic_root(def, &verdict.row.run_id),
        },
    )
    .await;
}

pub struct Finished<'a> {
    pub label: &'a str,
    pub run: &'a str,
    pub command: &'a str,
    pub commit: &'a str,
    pub passed: bool,
    pub source_verified: bool,
    pub output: &'a str,
    pub root: Option<String>,
}

async fn project_of(persist: &PersistHandle, def: &FleetDefinition, run: &str) -> Option<i64> {
    let sessions = task_sessions(persist, def, run).await;
    for session in sessions.into_values() {
        if let Ok(Some(identity)) = persist.session_identity_by_uuid(session).await {
            return Some(identity.project_id);
        }
    }
    None
}

fn failing_paths(def: &FleetDefinition, finished: &Finished<'_>) -> Vec<String> {
    if finished.passed {
        return Vec::new();
    }
    let found = match &finished.root {
        Some(root) => super::diagnostics::parse(finished.output, root),
        None => found_in(def, finished.output),
    };
    found.into_iter().map(|d| d.path).collect()
}

pub async fn record_finished(
    persist: &PersistHandle,
    def: &FleetDefinition,
    finished: Finished<'_>,
) {
    if !crate::change_record::writers::change_record_enabled(persist).await {
        return;
    }
    let Some(project_id) = project_of(persist, def, finished.run).await else {
        return;
    };
    let failing = failing_paths(def, &finished);
    let facts = crate::change_record::writers::CheckFacts {
        project_id,
        checkout: def.repo.clone(),
        run_id: Some(finished.run.to_string()),
        label: finished.label.to_string(),
        command: finished.command.to_string(),
        commit: finished.commit.to_string(),
        passed: finished.passed,
        source_verified: finished.source_verified,
        failing_paths: failing,
        at: chrono::Utc::now(),
    };
    if let Err(err) = crate::change_record::writers::record_check(persist, facts).await {
        tracing::warn!(error = %err, "change record: could not record a finished check");
    }
}

fn unfinished(report: &CheckReport) -> Option<String> {
    match report.how.as_str() {
        super::check_run::completion::RECORDING => Some(format!(
            "`{}` has ended; its captured result is waiting for database recovery. The command will not be rerun. Mike mails the result after it is recorded",
            report.command
        )),
        super::check_run::completion::BLOCKED => Some(format!(
            "`{}` has ended, but recording its result is blocked. Ask for status to see the persistence error and captured output location",
            report.command
        )),
        super::check::WAITING => Some(format!(
            "`{}` is waiting for the shared build slot (receipt {}){}. Mike mails the actual exit status and result when it ends; carry on without polling",
            report.command,
            report
                .receipt_id
                .map(|id| id.to_string())
                .unwrap_or_default(),
            report
                .queue
                .as_ref()
                .map(|place| format!(", {place}"))
                .unwrap_or_default()
        )),
        super::check::RUNNING => Some(format!(
            "`{}` is running at commit {} (state {}), for {} s so far{}. If you asked for the check, mike mails you its result when it ends: carry on with work that needs no build and do not wait in a loop",
            report.command,
            short_commit(&report.commit),
            short(&report.state),
            report.seconds,
            report
                .started_at
                .map(|at| format!(" (since {})", at.format("%H:%M:%SZ")))
                .unwrap_or_default()
        )),
        super::check::NOT_RUN if report.baseline => Some(format!(
            "`{}` has not run at commit {} (state {}) and is not running now; ask again without the status form to start this baseline",
            report.command,
            short_commit(&report.commit),
            short(&report.state)
        )),
        super::check::NOT_RUN => Some(format!(
            "`{}` has not run at commit {} (state {}) and is not running now; mike starts it by itself within a minute of a commit, or ask again without the status form to start it now",
            report.command,
            short_commit(&report.commit),
            short(&report.state)
        )),
        _ => None,
    }
}

fn listed(title: &str, found: &[Diagnostic], lines: &mut Vec<String>) {
    if found.is_empty() {
        return;
    }
    lines.push(title.to_string());
    lines.extend(
        found
            .iter()
            .map(|d| format!("  {}:{} {}", d.path, d.line, d.text)),
    );
}

pub fn text(report: &CheckReport) -> String {
    if let Some(why) = &report.refused {
        return format!("refused: {why}");
    }
    if let Some(waiting) = unfinished(report) {
        return if report.baseline {
            format!("Baseline: {waiting}")
        } else {
            waiting
        };
    }
    let mut lines = vec![format!(
        "`{}` {} at commit {} (state {}; {}, {} s, asked by {})",
        report.command,
        if report.passed {
            "passed"
        } else if report.how == super::nextest_filter::RAN_NO_TEST {
            "ran no test, so it neither passed nor failed,"
        } else {
            "failed"
        },
        short_commit(&report.commit),
        short(&report.state),
        report.how,
        report.seconds,
        report.askers.join(", ")
    )];
    if report.command_receipt && !report.baseline {
        lines.push("Command result from the shared checkout, including uncommitted edits; not commit or acceptance proof.".into());
    }
    listed("in your files:", &report.mine, &mut lines);
    if !report.others.is_empty() {
        lines.push(
            if report.baseline {
                "in other seats' files at this baseline:"
            } else if report.command_receipt {
                "in other seats' files:"
            } else {
                "in other seats' files (that seat has been told):"
            }
            .to_string(),
        );
        lines.extend(report.others.iter().map(|b| {
            format!(
                "  {}:{} [{} / {}] {}",
                b.path, b.line, b.seat, b.task, b.text
            )
        }));
    }
    listed("in files no task owns:", &report.unowned, &mut lines);
    let nothing_placed =
        report.mine.is_empty() && report.others.is_empty() && report.unowned.is_empty();
    if !report.passed && nothing_placed {
        lines.push("last lines of the output:".to_string());
        lines.extend(report.output_tail.lines().map(|l| format!("  {l}")));
    }
    let text = lines.join("\n");
    if report.baseline {
        format!("Baseline: {text}")
    } else {
        text
    }
}
