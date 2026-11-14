use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::test_support::{seed_fleet_done, seed_fleet_running};
use chrono::{Duration, Utc};
use serde_json::json;
use uuid::Uuid;

use super::definition::FleetDefinition;
use super::plan::ControlSeats;
use super::plan_tests::facts;
use super::retrospective::{Job, Phase};
use super::retrospective_worker::{failed, observe};
use super::tests::{changed, complete};
use crate::store::persistence::launch::LaunchRow;

fn job() -> Job {
    Job::new(
        "source-run",
        &complete(),
        json!([{"seat": "worker", "session": Uuid::nil()}]),
    )
}

fn launch() -> anyhow::Result<LaunchRow> {
    Ok(serde_json::from_value(json!({
        "id": 1, "backend": "herdr", "folder": "/srv/project", "worktree": false,
        "claude_args": [], "prompt_present": true, "remote_control": true,
        "proxy_env": true, "opened": true, "confirmed": true, "result": "opened",
        "created_at": Utc::now(), "pid": 42, "stderr_truncated": false,
    }))?)
}

#[test]
fn the_retrospective_is_on_by_default_and_a_definition_may_turn_it_off() {
    let def = changed(json!({}));
    assert!(def.retrospective.enabled);
    assert!(def.retrospective.write.is_none());
    let off = changed(json!({"retrospective": {"enabled": false}}));
    assert!(!off.retrospective.enabled);
}

#[test]
fn a_fleet_records_no_observer_and_opens_no_control_seat() {
    let def = changed(json!({
        "observer": null,
        "coordinator": {"session": Uuid::from_u128(1)},
        "acceptance": {"session": Uuid::from_u128(2)},
    }));
    assert_eq!(
        ControlSeats::recorded(&def).map(|seats| seats.observer),
        Some(None)
    );
    assert_eq!(super::environment::starts(&def).control_seats, 0);
    assert!(
        super::check_definition(&def)
            .iter()
            .all(|missing| !missing.item.starts_with("observer"))
    );
}

#[test]
fn missing_observer_does_not_disable_coordinator_digests() {
    let mut seats = facts().seats;
    seats.observer = None;
    let status = super::status::build(
        super::status::FleetHeader {
            fleet: "f".to_string(),
            slug: "s".to_string(),
            state: "running".to_string(),
            run_id: Some("r".to_string()),
            started_at: None,
            finished_at: None,
            now: Utc::now(),
            missing: Vec::new(),
        },
        &changed(json!({})),
        &[],
        &[],
    );
    let sent = super::tell_digest::digests(
        &status,
        &seats,
        (Default::default(), Default::default()),
        |_| false,
    );
    assert_eq!(sent.len(), 2);
    assert!(sent.iter().any(|notice| notice.to == seats.coordinator));
}

#[test]
fn writes_need_bounded_scope_and_a_separate_branch() {
    let def = changed(
        json!({"retrospective": {"enabled": true, "write": {"scope": "", "branch": "main"}}}),
    );
    assert_eq!(super::retrospective::validate(&def).len(), 2);
    let def = changed(
        json!({"retrospective": {"enabled": true, "write": {"scope": "Edit and test .claude/skills only; no publish", "branch": "retro/skills"}}}),
    );
    assert!(super::retrospective::validate(&def).is_empty());
}

#[test]
fn snapshot_has_independent_identity_and_no_nested_previous_job() -> anyhow::Result<()> {
    let mut definition = complete();
    definition["retrospective_job"] = json!({"old": true});
    let original = Job::new(
        "source-run",
        &definition,
        json!([{"seat": "closed-worker"}]),
    );
    let restored: Job = serde_json::from_value(json!(original))?;
    assert_eq!(original, restored);
    assert_ne!(original.run_id, original.source_run);
    assert!(
        original
            .source_definition
            .get("retrospective_job")
            .is_none()
    );
    assert_eq!(original.roster[0]["seat"], "closed-worker");
    Ok(())
}

#[test]
fn default_launch_has_no_edit_tools_hooks_or_inherited_settings() -> anyhow::Result<()> {
    let def = FleetDefinition::default();
    let args = super::retrospective_launch::args(&def, &job());
    assert!(
        args.windows(2)
            .any(|pair| pair == ["--tools", "Read,Glob,Grep,Bash"])
    );
    assert!(
        args.windows(2)
            .any(|pair| pair == ["--setting-sources", ""])
    );
    assert!(
        args.windows(2)
            .any(|pair| pair == ["--permission-mode", "dontAsk"])
    );
    let settings = args
        .last()
        .ok_or_else(|| anyhow::anyhow!("missing settings argument"))?;
    assert!(settings.contains("\"disableAllHooks\":true"));
    assert!(settings.contains("\"MIKE_MCP_PROFILE\":\"retrospective\""));
    assert!(!settings.contains("Bash(*"));
    assert!(!settings.contains("send_session_message"));
    Ok(())
}

#[test]
fn live_or_previously_opened_launch_is_not_relaunched() -> anyhow::Result<()> {
    let mut job = job();
    job.phase = Phase::Launching;
    let mut row = launch()?;
    observe(&mut job, Some(&row), |_| true);
    assert_eq!(job.phase, Phase::Running);
    row.closed_at = Some(Utc::now());
    observe(&mut job, Some(&row), |_| true);
    assert_eq!(job.phase, Phase::Running);
    observe(&mut job, Some(&row), |_| false);
    assert_eq!(job.phase, Phase::Cleanup);
    Ok(())
}

#[test]
fn ambiguous_launch_is_visible_and_retains_checkout_without_retry() -> anyhow::Result<()> {
    let mut job = job();
    let mut row = launch()?;
    row.opened = false;
    row.pid = None;
    row.result = "starting".to_string();
    observe(&mut job, Some(&row), |_| false);
    assert_eq!(job.phase, Phase::Blocked);
    assert!(job.retains_checkout());
    assert!(job.problem.is_some());
    Ok(())
}

#[test]
fn failed_launch_without_exit_proof_is_blocked_at_every_attempt() -> anyhow::Result<()> {
    let mut row = launch()?;
    row.opened = false;
    row.pid = None;
    row.result = "failed".to_string();
    row.reason = Some("open_failed".to_string());
    for attempts in [1, 2, 3] {
        let mut job = job();
        let session = job.session;
        job.phase = Phase::Launching;
        job.attempts = attempts;
        observe(&mut job, Some(&row), |_| false);
        assert_eq!(job.phase, Phase::Blocked);
        assert_eq!(job.session, session);
        assert_eq!(job.attempts, attempts);
        assert!(job.retains_checkout());
        assert!(!super::retrospective_worker::safe_to_cleanup(&row, |_| {
            false
        }));
    }
    Ok(())
}

#[test]
fn retries_back_off_are_bounded_and_permanent_errors_do_not_retry() {
    let mut job = job();
    job.attempts = 1;
    failed(&mut job, "offline".to_string(), true);
    assert_eq!(job.phase, Phase::Pending);
    assert!(job.next_at > Utc::now() + Duration::seconds(100));
    job.attempts = 3;
    failed(&mut job, "offline".to_string(), true);
    assert_eq!(job.phase, Phase::Blocked);
    job.attempts = 1;
    failed(&mut job, "not authorized".to_string(), false);
    assert_eq!(job.phase, Phase::Cleanup);
    job.phase = Phase::Failed;
    assert!(!job.retains_checkout());
}

#[tokio::test]
async fn closing_queues_one_durable_job_and_only_done_fleets_dispatch() -> anyhow::Result<()> {
    use crate::store::persistence::fleet::NewFleet;
    use crate::store::persistence::launch::NewRun;
    use crate::store::persistence::{PersistHandle, test_support};
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; retrospective lifecycle DB test NOT RUN");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let source = format!("source-{}", Uuid::new_v4());
    persist
        .launch()
        .upsert_run(&NewRun {
            run_id: source.clone(),
            project_path: Some("/tmp/retro".to_string()),
            backend: "herdr".to_string(),
            workspace_ref: None,
            tab_ref: None,
        })
        .await?;
    let row = persist
        .fleet()
        .insert_fleet(&NewFleet {
            slug: format!("retro-{}", Uuid::new_v4()),
            project_path: "/tmp/retro".to_string(),
            definition: json!({"repo": "/tmp/retro", "retrospective": {"enabled": true}}),
            missing: json!([]),
            created_by: None,
        })
        .await?;
    let row = seed_fleet_running(&persist, row.fleet_uuid, &source).await?;
    let row = persist
        .fleet()
        .set_fleet_state(row.fleet_uuid, "stopping", None, None)
        .await?;
    let (first, second) = tokio::join!(
        super::retrospective::stage(&persist, &row, &source),
        super::retrospective::stage(&persist, &row, &source)
    );
    assert!(first?);
    assert!(second?);
    let queued = persist
        .fleet()
        .find_fleet(&row.fleet_uuid.to_string())
        .await?
        .ok_or_else(|| anyhow::anyhow!("queued fleet missing"))?;
    let original: Job = serde_json::from_value(queued.definition["retrospective_job"].clone())?;
    assert_eq!(original.source_run, source);
    assert_eq!(original.phase, Phase::Pending);
    assert!(
        persist
            .fleet()
            .pending_fleet_retrospectives()
            .await?
            .iter()
            .all(|item| item.fleet_uuid != row.fleet_uuid)
    );
    seed_fleet_done(&persist, row.fleet_uuid).await?;
    let ready = persist.fleet().pending_fleet_retrospectives().await?;
    let ready = ready
        .iter()
        .find(|item| item.fleet_uuid == row.fleet_uuid)
        .ok_or_else(|| anyhow::anyhow!("done retrospective missing"))?;
    assert_eq!(ready.definition["retrospective_job"], json!(original));
    let mut claimed = original.clone();
    claimed.phase = Phase::Launching;
    assert!(
        super::retrospective::save(&persist, ready, &claimed)
            .await?
            .is_some()
    );
    assert!(
        super::retrospective::save(&persist, ready, &claimed)
            .await?
            .is_none()
    );
    assert!(super::retrospective::stage(&persist, &row, &source).await?);
    let recovered = persist
        .fleet()
        .find_fleet(&row.fleet_uuid.to_string())
        .await?
        .ok_or_else(|| anyhow::anyhow!("recovered fleet missing"))?;
    assert_eq!(
        recovered.definition["retrospective_job"]["session"],
        json!(original.session)
    );
    db.cleanup().await
}

#[test]
fn cleanup_waits_for_a_known_end_even_without_a_pid() -> anyhow::Result<()> {
    let mut row = launch()?;
    assert!(!super::retrospective_worker::safe_to_cleanup(&row, |_| {
        true
    }));
    assert!(super::retrospective_worker::safe_to_cleanup(&row, |_| {
        false
    }));
    row.pid = None;
    assert!(!super::retrospective_worker::safe_to_cleanup(&row, |_| {
        false
    }));
    row.closed_at = Some(Utc::now());
    assert!(!super::retrospective_worker::safe_to_cleanup(&row, |_| {
        false
    }));
    row.close_process_exited = Some(false);
    assert!(!super::retrospective_worker::safe_to_cleanup(&row, |_| {
        false
    }));
    row.close_process_exited = Some(true);
    assert!(super::retrospective_worker::safe_to_cleanup(&row, |_| {
        false
    }));
    Ok(())
}

#[test]
fn closed_work_tripwire_ignores_a_live_replacement_and_ready_handoffs() {
    let def = changed(json!({}));
    let mut status = super::status::build(
        super::status::FleetHeader {
            fleet: "fleet".to_string(),
            slug: "s".to_string(),
            state: "running".to_string(),
            run_id: Some("r".to_string()),
            started_at: None,
            finished_at: None,
            now: Utc::now(),
            missing: Vec::new(),
        },
        &def,
        &[],
        &[],
    );
    let seat = super::status::SeatStatus {
        seat: "db-dev-P1".to_string(),
        role: "db-dev".to_string(),
        model: "haiku".to_string(),
        state: "closed".to_string(),
        session: Some(Uuid::new_v4()),
        goal: None,
        context_tokens: None,
        requests: 0,
        last_request_at: None,
        opened_at: None,
        closed: true,
        first_request_cache: None,
    };
    status.seats.push(seat.clone());
    status.tasks[0].seat = None;
    status.tasks[0].last_seat = Some("db-dev-P1".to_string());
    assert_eq!(
        super::tell_tripwires::closed_with_work(&status, Uuid::nil()).len(),
        1
    );
    status.seats.push(super::status::SeatStatus {
        closed: false,
        ..seat
    });
    status.tasks[0].seat = Some("db-dev-P1".to_string());
    assert!(super::tell_tripwires::closed_with_work(&status, Uuid::nil()).is_empty());
    status.seats.pop();
    status.tasks[0].seat = None;
    status.tasks[0].ready = true;
    assert!(super::tell_tripwires::closed_with_work(&status, Uuid::nil()).is_empty());
}
