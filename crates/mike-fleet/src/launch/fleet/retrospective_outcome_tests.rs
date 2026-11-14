use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::test_support::{seed_project, seed_two_sessions};
use crate::store::persistence::work_threads::{CheckpointInput, EntryInput, MetaUpdate};
use crate::store::persistence::{PersistHandle, test_support};
use serde_json::json;
use uuid::Uuid;

use super::definition::FleetDefinition;
use super::retrospective::{Job, Phase, Retrospective, save};
use super::retrospective_outcome::{findings, mail, problem};
use super::retrospective_worker::cleanup;
use crate::store::persistence::fleet::NewFleet;
use crate::store::persistence::launch::NewRun;

#[test]
fn no_turns_is_a_failure_with_the_reason_even_when_nothing_else_is_known() {
    let reason = problem(0, 0, "fleet-runs").unwrap_or_default();
    assert!(reason.contains("no turns"), "{reason}");
    assert!(problem(0, 3, "fleet-runs").is_some());
}

#[test]
fn turns_without_a_finding_name_the_thread_that_stayed_empty() {
    let reason = problem(7, 0, "fleet-runs").unwrap_or_default();
    assert!(reason.contains("7 turns"), "{reason}");
    assert!(reason.contains("`fleet-runs`"), "{reason}");
    assert_eq!(problem(7, 1, "fleet-runs"), None);
}

#[test]
fn the_defaults_run_on_sonnet_and_write_to_the_fleet_runs_thread() {
    let defaults = Retrospective::default();
    assert_eq!(defaults.model, "sonnet");
    assert_eq!(defaults.thread, "fleet-runs");
    assert_eq!(defaults.issues_repo.as_deref(), Some("temet-ai/mike"));
    assert_eq!(defaults.questions.len(), 4);
    assert!(defaults.write.is_none());
    let parsed = FleetDefinition::from_value(&json!({"retrospective": {"enabled": true}}));
    assert_eq!(
        parsed.map(|def| def.retrospective),
        Ok(defaults),
        "a definition that omits the block gets the defaults"
    );
}

#[test]
fn the_definition_overrides_model_questions_and_where_it_writes() -> anyhow::Result<()> {
    let def = FleetDefinition::from_value(&json!({"retrospective": {
        "model": "opus", "questions": ["What slowed you down?"],
        "thread": "research-runs", "issues_repo": null,
    }}))
    .map_err(anyhow::Error::msg)?;
    assert_eq!(def.retrospective.model, "opus");
    assert_eq!(def.retrospective.questions, ["What slowed you down?"]);
    assert_eq!(def.retrospective.thread, "research-runs");
    assert_eq!(def.retrospective.issues_repo, None);
    let job = Job::new("run", &json!({}), json!([]));
    let args = super::retrospective_launch::args(&def, &job);
    assert!(args.windows(2).any(|pair| pair == ["--model", "opus"]));
    assert!(
        !args
            .last()
            .is_some_and(|settings| settings.contains("gh issue"))
    );
    Ok(())
}

#[test]
fn the_default_launch_can_debrief_and_write_but_not_edit_code() -> anyhow::Result<()> {
    let def = FleetDefinition::default();
    let job = Job::new("run", &json!({}), json!([]));
    let args = super::retrospective_launch::args(&def, &job);
    assert!(args.windows(2).any(|pair| pair == ["--model", "sonnet"]));
    assert!(
        args.windows(2)
            .any(|pair| pair == ["--tools", "Read,Glob,Grep,Bash"])
    );
    let settings = args.last().ok_or_else(|| anyhow::anyhow!("no settings"))?;
    for allowed in [
        "Bash(claude -p --resume:*)",
        "Bash(gh issue create:*)",
        "mcp__mike__work_thread_add_entry",
        "mcp__mike__work_thread_checkpoint",
    ] {
        assert!(settings.contains(allowed), "{allowed} missing: {settings}");
    }
    for edit in ["\"Edit", "\"Write", "Bash(*", "Bash(git", "Bash(rm"] {
        assert!(!settings.contains(edit), "{edit} present: {settings}");
    }
    Ok(())
}

#[test]
fn the_prompt_carries_the_fixed_questions_the_thread_and_the_fork_command() -> anyhow::Result<()> {
    let def = FleetDefinition::default();
    let job = Job::new("run", &json!({}), json!([{"seat": "worker"}]));
    let row = fleet_row("prompted", None);
    let prompt = super::retrospective_launch::prompt(&row, &def, &job);
    assert!(prompt.contains("claude -p --resume <seat-session> --fork-session --tools \"\""));
    assert!(prompt.contains("effort between real work on your task and fleet machinery"));
    assert!(prompt.contains("system prompt of about ten lines"));
    assert!(prompt.contains("work thread `fleet-runs`"));
    assert!(prompt.contains("kind `finding`"));
    assert!(prompt.contains("gh issue create -R temet-ai/mike"));
    Ok(())
}

fn fleet_row(slug: &str, created_by: Option<Uuid>) -> crate::store::persistence::fleet::FleetRow {
    crate::store::persistence::fleet::FleetRow {
        id: 1,
        fleet_uuid: Uuid::new_v4(),
        slug: slug.to_string(),
        project_path: "/repo".to_string(),
        state: "done".to_string(),
        definition: json!({}),
        missing: json!([]),
        run_id: None,
        start_report: None,
        created_by,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        ready_at: None,
        started_at: None,
        finished_at: None,
    }
}

#[test]
fn the_author_mail_names_the_fleet_the_reason_and_the_thread() {
    let row = fleet_row("open-issues", Some(Uuid::new_v4()));
    let text = mail(&row, &FleetDefinition::default(), "no turns");
    assert!(
        text.starts_with("[mike] The retrospective of fleet open-issues ended failed: no turns.")
    );
    assert!(text.contains("`fleet-runs`"));
}

#[tokio::test]
async fn a_retrospective_with_no_turns_ends_failed_with_the_reason_and_tells_the_author()
-> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; retrospective outcome DB test NOT RUN");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let repo = tempfile::tempdir()?;
    let repo_path = repo.path().to_string_lossy().to_string();
    let project = seed_project(persist.pool(), &repo_path).await?;
    let (author, silent) = (Uuid::new_v4(), Uuid::new_v4());
    seed_two_sessions(
        persist.pool(),
        &author.to_string(),
        &silent.to_string(),
        project,
    )
    .await?;
    let row = persist
        .fleet()
        .insert_fleet(&NewFleet {
            slug: format!("zero-turns-{}", Uuid::new_v4()),
            project_path: repo_path.clone(),
            definition: json!({"repo": repo_path}),
            missing: json!([]),
            created_by: Some(author),
        })
        .await?;
    let mut job = Job::new("source-run", &row.definition, json!([]));
    job.session = silent;
    job.run_id = format!("retro-{}", Uuid::new_v4());
    job.phase = Phase::Cleanup;
    persist
        .launch()
        .upsert_run(&NewRun {
            run_id: job.run_id.clone(),
            project_path: Some(repo_path.clone()),
            backend: "herdr".to_string(),
            workspace_ref: None,
            tab_ref: None,
        })
        .await?;
    let row = save(&persist, &row, &job)
        .await?
        .ok_or_else(|| anyhow::anyhow!("job not saved"))?;
    cleanup(&persist, &row, job).await?;
    let after = persist
        .fleet()
        .find_fleet(&row.fleet_uuid.to_string())
        .await?
        .ok_or_else(|| anyhow::anyhow!("fleet missing"))?;
    let recorded: Job = serde_json::from_value(after.definition["retrospective_job"].clone())?;
    assert_eq!(recorded.phase, Phase::Failed);
    let reason = recorded.problem.unwrap_or_default();
    assert!(reason.contains("no turns"), "{reason}");
    let told: Vec<String> = sqlx::query_scalar(
        "SELECT m.body FROM mailbox.messages m JOIN sessions.sessions s ON s.id = m.to_session_id WHERE s.session_uuid = $1",
    )
    .bind(author)
    .fetch_all(persist.pool())
    .await?;
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(told[0].contains("ended failed"), "{}", told[0]);
    assert!(told[0].contains("no turns"), "{}", told[0]);
    db.cleanup().await
}

#[tokio::test]
async fn only_finding_entries_by_the_retrospective_session_count() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; retrospective findings DB test NOT RUN");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let path = format!("/tmp/findings-{}", Uuid::new_v4());
    let project = seed_project(persist.pool(), &path).await?;
    let (retro, other) = (Uuid::new_v4(), Uuid::new_v4());
    seed_two_sessions(
        persist.pool(),
        &retro.to_string(),
        &other.to_string(),
        project,
    )
    .await?;
    let row = fleet_row("findings", None);
    let row = crate::store::persistence::fleet::FleetRow {
        project_path: path.clone(),
        ..row
    };
    let def = FleetDefinition::default();
    assert_eq!(findings(&persist, &row, &def, retro).await?, 0);
    persist
        .launch()
        .checkpoint_work_thread(
            project,
            &CheckpointInput {
                slug: "fleet-runs",
                tldr: "fleet runs",
                state: Default::default(),
                session_uuid: Some(retro),
                commit_sha: None,
                contribution: None,
                meta: MetaUpdate {
                    title: Some("Fleet runs"),
                    ..Default::default()
                },
            },
        )
        .await?;
    assert_eq!(findings(&persist, &row, &def, retro).await?, 0);
    for (session, kind) in [(other, "finding"), (retro, "note"), (retro, "finding")] {
        persist
            .launch()
            .add_work_thread_entry(
                project,
                &EntryInput {
                    slug: "fleet-runs",
                    kind,
                    body: "body",
                    summary: Some("summary"),
                    session_uuid: Some(session),
                    commit_sha: None,
                    data: None,
                },
            )
            .await?;
    }
    assert_eq!(findings(&persist, &row, &def, retro).await?, 1);
    db.cleanup().await
}
