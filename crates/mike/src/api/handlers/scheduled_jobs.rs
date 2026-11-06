use super::*;
use crate::scheduled_jobs::{JOBS, JobState, JobView, build_view, snapshot};
use uuid::Uuid;

const INERT_AFTER_FAILURES: u32 = 3;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ScheduledJobsResponse {
    pub(crate) machine_uuid: Option<Uuid>,
    pub(crate) machine_name: Option<String>,
    pub(crate) jobs: Vec<JobView>,
}

async fn machine_identity() -> Option<crate::machine_identity::MachineIdentity> {
    let dir = crate::machine_identity::default_state_dir().ok()?;
    match crate::machine_identity::load_from(&dir).await {
        Ok(identity) => Some(identity),
        Err(e) => {
            tracing::debug!(error = %e, "scheduled jobs: no machine identity to report");
            None
        }
    }
}

async fn lease_owners(state: &AppState) -> Vec<(String, Uuid)> {
    let Some(persist) = state.persist() else {
        return Vec::new();
    };
    match persist.list_llm_worker_leases().await {
        Ok(rows) => rows
            .into_iter()
            .map(|row| (row.role, row.machine_uuid))
            .collect(),
        Err(e) => {
            tracing::debug!(error = %e, "scheduled jobs: could not read the job leases");
            Vec::new()
        }
    }
}

async fn gather(state: &AppState) -> ScheduledJobsResponse {
    let identity = machine_identity().await;
    let owners = lease_owners(state).await;
    let now = chrono::Utc::now();
    let jobs = JOBS
        .iter()
        .map(|job| {
            let owner = owners
                .iter()
                .find(|(role, _)| role == job.name)
                .map(|(_, machine)| *machine);
            build_view(job, &snapshot(job.name), owner, now)
        })
        .collect();
    ScheduledJobsResponse {
        machine_uuid: identity.as_ref().map(|i| i.machine_uuid),
        machine_name: identity.map(|i| i.machine_name),
        jobs,
    }
}

pub(crate) async fn scheduled_jobs(State(state): State<AppState>) -> Json<ScheduledJobsResponse> {
    Json(gather(&state).await)
}

fn job_state(view: &JobView) -> CapabilityState {
    match view.state {
        JobState::Failing if view.consecutive_failures >= INERT_AFTER_FAILURES => {
            CapabilityState::Inert
        }
        JobState::Failing | JobState::Overdue => CapabilityState::Degraded,
        JobState::Off => CapabilityState::Off,
        JobState::Ok | JobState::NeverRun | JobState::Running => CapabilityState::Active,
    }
}

fn summary(view: &JobView) -> String {
    if !view.detail.is_empty() {
        return view.detail.clone();
    }
    match (view.state, view.last_finished_at) {
        (JobState::Running, _) => "running now".to_owned(),
        (JobState::Ok, Some(at)) => format!(
            "last run finished {} and succeeded; next {}",
            at.format("%Y-%m-%d %H:%M UTC"),
            view.next_run_at.map_or_else(
                || "run unknown".to_owned(),
                |next| next.format("%Y-%m-%d %H:%M UTC").to_string()
            )
        ),
        _ => format!("has not run yet; runs {}", view.schedule),
    }
}

fn worst(states: &[CapabilityState]) -> CapabilityState {
    for wanted in [CapabilityState::Inert, CapabilityState::Degraded] {
        if states.contains(&wanted) {
            return wanted;
        }
    }
    if states.iter().all(|s| *s == CapabilityState::Off) {
        CapabilityState::Off
    } else {
        CapabilityState::Active
    }
}

pub(crate) fn scheduled_jobs_row(views: &[JobView]) -> CapabilityRow {
    let states: Vec<CapabilityState> = views.iter().map(job_state).collect();
    let detail = views
        .iter()
        .map(|view| format!("{}: {}", view.label, summary(view)))
        .collect::<Vec<_>>()
        .join(" | ");
    let fix = views
        .iter()
        .filter(|view| !view.fix.is_empty())
        .map(|view| format!("{}: {}", view.label, view.fix))
        .collect::<Vec<_>>()
        .join(" | ");
    CapabilityRow {
        id: "scheduled_jobs",
        label: "Scheduled jobs",
        state: worst(&states),
        detail,
        fix,
    }
}

pub(crate) async fn scheduled_jobs_capability(state: &AppState) -> CapabilityRow {
    scheduled_jobs_row(&gather(state).await.jobs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(index: usize, state: JobState, failures: u32) -> JobView {
        let mut v = build_view(
            &JOBS[index],
            &snapshot("test_only"),
            None,
            chrono::Utc::now(),
        );
        v.state = state;
        v.consecutive_failures = failures;
        if matches!(state, JobState::Failing | JobState::Overdue) {
            v.detail = format!("{} went wrong", v.label);
            v.fix = "do the thing".to_owned();
        }
        v
    }

    #[test]
    fn healthy_jobs_make_the_row_active_and_list_every_job() {
        let views = vec![
            view(0, JobState::Ok, 0),
            view(1, JobState::NeverRun, 0),
            view(2, JobState::Running, 0),
            view(3, JobState::Ok, 0),
            view(4, JobState::Ok, 0),
            view(5, JobState::Ok, 0),
        ];
        let row = scheduled_jobs_row(&views);
        assert_eq!(row.id, "scheduled_jobs");
        assert_eq!(row.state, CapabilityState::Active);
        for job in JOBS.iter() {
            assert!(row.detail.contains(job.label), "{}", row.detail);
        }
        assert!(row.fix.is_empty());
    }

    #[test]
    fn one_failing_job_makes_the_row_degraded_and_names_it_with_its_fix() {
        let views = vec![view(0, JobState::Ok, 0), view(1, JobState::Failing, 1)];
        let row = scheduled_jobs_row(&views);
        assert_eq!(row.state, CapabilityState::Degraded);
        assert!(
            row.detail.contains("Weekly learning audit went wrong"),
            "{}",
            row.detail
        );
        assert!(
            row.fix.contains("Weekly learning audit: do the thing"),
            "{}",
            row.fix
        );
    }

    #[test]
    fn repeated_failures_make_the_row_inert() {
        let views = vec![view(0, JobState::Failing, 3), view(1, JobState::Ok, 0)];
        assert_eq!(scheduled_jobs_row(&views).state, CapabilityState::Inert);
    }

    #[test]
    fn an_overdue_job_makes_the_row_degraded() {
        let views = vec![view(3, JobState::Overdue, 0)];
        assert_eq!(scheduled_jobs_row(&views).state, CapabilityState::Degraded);
    }

    #[test]
    fn the_row_is_off_only_when_every_job_is_off() {
        let all_off = vec![view(0, JobState::Off, 0), view(1, JobState::Off, 0)];
        assert_eq!(scheduled_jobs_row(&all_off).state, CapabilityState::Off);
        let some_off = vec![view(0, JobState::Off, 0), view(1, JobState::Ok, 0)];
        assert_eq!(scheduled_jobs_row(&some_off).state, CapabilityState::Active);
    }

    #[test]
    fn a_failing_job_is_never_reported_active() {
        let views = vec![view(2, JobState::Failing, 1)];
        assert_ne!(scheduled_jobs_row(&views).state, CapabilityState::Active);
    }
}
