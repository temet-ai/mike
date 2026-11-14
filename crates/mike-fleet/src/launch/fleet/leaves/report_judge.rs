use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use uuid::Uuid;

use super::judge_jev::jev_judge;
use super::{
    CriterionText, EarlierVerdict, JudgeAnswer, JudgeInput, JudgeVerdict, LeafError, LeafModel,
    ReportFacts, SharedTask, judge, verdict_events,
};
use crate::launch::fleet::acceptance_register::register;
use crate::launch::fleet::definition::FleetDefinition;
use crate::launch::fleet::events::{EventKind, FleetEvent};
use crate::launch::fleet::floor::Outcome;
use crate::launch::fleet::problems::{self, Trigger};
use crate::launch::fleet::report::Exercised;
use crate::launch::fleet::status::note_facts;
use crate::launch::fleet::watch::git;
use crate::llm::decisions::point::Decider;
use crate::store::persistence::PersistHandle;

const PATCH_BYTES: usize = 1 << 20;

#[derive(Debug, Clone)]
pub struct ReportToJudge {
    pub run: String,
    pub fleet: String,
    pub task: String,
    pub seat: Uuid,
    pub note: Uuid,
    pub sha: String,
    pub criteria: Vec<String>,
    pub exercised: Option<Exercised>,
    pub tests_changed: Vec<String>,
    pub unestablished: Vec<String>,
}

pub const FAILS_BEFORE_A_PROBLEM: usize = 3;

pub async fn judge_report(
    model: &dyn LeafModel,
    persist: &PersistHandle,
    def: &FleetDefinition,
    report: &ReportToJudge,
    floor: Outcome,
) -> Result<(), LeafError> {
    judge_report_with(None, model, persist, def, (report, floor)).await
}

pub async fn judge_report_with(
    decider: Option<&Decider>,
    model: &dyn LeafModel,
    persist: &PersistHandle,
    def: &FleetDefinition,
    (report, floor): (&ReportToJudge, Outcome),
) -> Result<(), LeafError> {
    let events = persist
        .fleet()
        .fleet_events_since(&report.run, 0)
        .await
        .unwrap_or_default();
    let earlier = earlier_rounds(&events, &report.task);
    let input = input_for(persist, def, report, floor, earlier.clone()).await;
    let by_jev = match decider {
        Some(decider) => jev_judge(decider, &input, report.note).await,
        None => None,
    };
    let judged = match by_jev {
        Some(judged) => judged,
        None => judge(model, &input).await?,
    };
    record(persist, report, &judged.answer).await;
    open_problem_on_repeated_fails(persist, report, &judged.answer, &earlier).await;
    Ok(())
}

pub fn earlier_rounds(events: &[FleetEvent], task: &str) -> Vec<EarlierVerdict> {
    let mut by_note: Vec<(Uuid, EarlierVerdict)> = Vec::new();
    let mut ordered: Vec<&FleetEvent> = events
        .iter()
        .filter(|e| e.kind == EventKind::VerdictRecorded && e.task.as_deref() == Some(task))
        .collect();
    ordered.sort_by_key(|e| (e.occurred_at, e.id));
    for event in ordered {
        let Some(note) = event
            .payload
            .get("note_uuid")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<Uuid>().ok())
        else {
            continue;
        };
        let passed = event.payload.get("passed").and_then(|v| v.as_bool()) == Some(true);
        let criterion = event
            .payload
            .get("criterion")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let needed = event
            .payload
            .get("needed")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        match by_note.iter_mut().find(|(known, _)| *known == note) {
            Some((_, verdict)) => {
                verdict.passed &= passed;
                verdict.criteria.push(criterion);
                if verdict.needed.is_empty() {
                    verdict.needed = needed;
                }
            }
            None => by_note.push((
                note,
                EarlierVerdict {
                    round: by_note.len() + 1,
                    passed,
                    criteria: vec![criterion],
                    needed,
                },
            )),
        }
    }
    by_note.into_iter().map(|(_, verdict)| verdict).collect()
}

pub fn repeatedly_failed(answer: &JudgeAnswer, earlier: &[EarlierVerdict]) -> Vec<String> {
    if answer.verdict != JudgeVerdict::Fail {
        return Vec::new();
    }
    answer
        .criteria
        .iter()
        .filter(|criterion| {
            let before = earlier
                .iter()
                .filter(|v| !v.passed && v.criteria.contains(criterion))
                .count();
            before + 1 >= FAILS_BEFORE_A_PROBLEM
        })
        .cloned()
        .collect()
}

async fn open_problem_on_repeated_fails(
    persist: &PersistHandle,
    report: &ReportToJudge,
    answer: &JudgeAnswer,
    earlier: &[EarlierVerdict],
) {
    let failed = repeatedly_failed(answer, earlier);
    if failed.is_empty() {
        return;
    }
    let needed: Vec<&str> = earlier
        .iter()
        .filter(|v| !v.passed)
        .map(|v| v.needed.as_str())
        .chain(answer.needed.as_deref())
        .filter(|n| !n.trim().is_empty())
        .collect();
    let detail = format!(
        "task {} has failed {} on {} rounds; what each round needed: {}",
        report.task,
        failed.join(", "),
        earlier.iter().filter(|v| !v.passed).count() + 1,
        needed.join(" | ")
    );
    if let Err(err) = problems::open(
        persist,
        &report.run,
        Trigger::CriterionFailedThrice,
        Some(&report.task),
        &detail,
    )
    .await
    {
        tracing::warn!(error = %err, task = %report.task, "fleet judge: could not open the repeated-fail problem");
    }
}

async fn record(persist: &PersistHandle, report: &ReportToJudge, answer: &JudgeAnswer) {
    for event in verdict_events(&report.run, &report.task, None, report.note, answer) {
        if let Err(err) = persist.fleet().append_fleet_event(&event).await {
            tracing::warn!(error = %err, task = %report.task, "fleet judge: could not record a verdict");
        }
    }
    tell_seat(persist, report.seat, &verdict_mail(report, answer)).await;
}

pub async fn tell_seat(persist: &PersistHandle, seat: Uuid, body: &str) {
    if let Err(err) = persist
        .launch()
        .insert_message(Uuid::new_v4(), seat, None, body)
        .await
    {
        tracing::warn!(error = %err, %seat, "fleet judge: could not mail the seat");
    }
}

fn verdict_mail(report: &ReportToJudge, answer: &JudgeAnswer) -> String {
    let word = if answer.verdict == JudgeVerdict::Pass {
        "PASS"
    } else {
        "FAIL"
    };
    let short = report.sha.get(..10).unwrap_or(&report.sha);
    let mut text = format!(
        "[mike judge] {word} task {} {} at {short}: {}",
        report.task,
        answer.criteria.join(", "),
        answer.observation.trim()
    );
    if let Some(needed) = answer
        .needed
        .as_deref()
        .map(str::trim)
        .filter(|needed| !needed.is_empty())
    {
        text.push_str("\nNeeded before a pass: ");
        text.push_str(needed);
    }
    if !answer.evidence_read.is_empty() {
        text.push_str("\nEvidence read: ");
        text.push_str(&answer.evidence_read.join(", "));
    }
    text
}

async fn input_for(
    persist: &PersistHandle,
    def: &FleetDefinition,
    report: &ReportToJudge,
    floor: Outcome,
    earlier_verdicts: Vec<EarlierVerdict>,
) -> JudgeInput {
    let criteria = report
        .criteria
        .iter()
        .filter_map(|id| {
            let criterion = def.criteria.get(id)?;
            Some(CriterionText {
                id: id.clone(),
                text: criterion.text.clone(),
                check: criterion.check.clone(),
            })
        })
        .collect();
    JudgeInput {
        task: report.task.clone(),
        criteria,
        report: facts(report),
        floor,
        diff: patch_of(&def.repo, &report.sha).await,
        open_obligations: open_obligations(persist, def, report).await,
        task_brief: task_brief(def, &report.task),
        shared_with: shared_with(def, &report.task, &report.criteria),
        earlier_verdicts,
    }
}

fn task_brief(def: &FleetDefinition, task: &str) -> String {
    def.tasks
        .get(task)
        .map(|t| format!("{}: {}", t.name, t.brief))
        .unwrap_or_default()
}

fn shared_with(def: &FleetDefinition, task: &str, criteria: &[String]) -> Vec<SharedTask> {
    def.tasks
        .iter()
        .filter(|(id, _)| id.as_str() != task)
        .filter_map(|(id, other)| {
            let shared: Vec<String> = other
                .criteria
                .iter()
                .filter(|c| criteria.contains(c))
                .cloned()
                .collect();
            (!shared.is_empty()).then(|| SharedTask {
                task: id.clone(),
                name: other.name.clone(),
                criteria: shared,
            })
        })
        .collect()
}

fn facts(report: &ReportToJudge) -> ReportFacts {
    let exercised = report.exercised.clone().unwrap_or_default();
    ReportFacts {
        commit: report.sha.clone(),
        criteria: report.criteria.clone(),
        exercised_command: exercised.command,
        exercised_exit: exercised.exit,
        exercised_output_ref: exercised.output_ref,
        tests_changed: report.tests_changed.clone(),
        unestablished: report.unestablished.clone(),
    }
}

async fn patch_of(repo: &str, sha: &str) -> String {
    let args = [
        "show",
        "--no-color",
        "--stat",
        "--patch",
        "--format=%H %s",
        sha,
    ];
    let Some(raw) = git(repo, &args, None).await else {
        return String::new();
    };
    let text = String::from_utf8_lossy(&raw).into_owned();
    if text.len() <= PATCH_BYTES {
        return text;
    }
    let end = (0..=PATCH_BYTES)
        .rev()
        .find(|i| text.is_char_boundary(*i))
        .unwrap_or(0);
    format!(
        "{}\n[the patch is longer than {PATCH_BYTES} bytes; the rest is in commit {sha}]",
        &text[..end]
    )
}

async fn open_obligations(
    persist: &PersistHandle,
    def: &FleetDefinition,
    report: &ReportToJudge,
) -> Vec<String> {
    let rows = persist
        .fleet()
        .fleet_note_rows(&report.run)
        .await
        .unwrap_or_default();
    let notes: Vec<_> = rows.into_iter().map(note_facts).collect();
    register(&report.fleet, def, &notes)
        .into_iter()
        .filter(|obligation| {
            obligation.required
                && obligation.tasks.iter().any(|task| task == &report.task)
                && !obligation.accepted_for(&report.task)
        })
        .map(|obligation| {
            format!(
                "{} ({}): {}",
                obligation.obligation_id, obligation.outcome, obligation.title
            )
        })
        .collect()
}
