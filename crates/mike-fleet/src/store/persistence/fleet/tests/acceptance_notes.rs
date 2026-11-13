use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::test_support::seed_fleet_running;
use serde_json::json;
use uuid::Uuid;

use super::seed::{seed_run, seed_session};
use super::{Fixture, fixture, new_fleet};
use crate::store::persistence::launch::{LaunchSurfaceRefs, NewLaunch};
use crate::store::persistence::session_goals::SetSessionIdentity;
use crate::store::persistence::session_notes::SessionNoteRow;
use crate::store::persistence::test_support::seed_project;

async fn acceptance_launch(f: &Fixture, run: &str, session: Uuid) -> anyhow::Result<i64> {
    let mut launch = NewLaunch::new("herdr", "/tmp/fleet-a");
    launch.run_id = Some(run.to_owned());
    launch.session_uuid = Some(session);
    launch.seat = Some("acceptance".to_owned());
    launch.identity = Some(json!({"role": "acceptance"}));
    let id = f.handle.launch().insert_launch(&launch).await?;
    f.handle
        .launch()
        .record_launch_opened(id, &LaunchSurfaceRefs::default())
        .await?;
    Ok(id)
}

async fn seed_legacy_obligation(
    f: &Fixture,
    session: Uuid,
    title: &str,
    tags: &[String],
) -> anyhow::Result<SessionNoteRow> {
    Ok(sqlx::query_as::<_, SessionNoteRow>(
        "INSERT INTO workspace.session_notes (session_id, kind, title, body, status, tags)
         SELECT id, 'task', $2, 'Exercise the actual setting and correlate its state change.', 'open', $3
           FROM sessions.sessions WHERE session_uuid = $1
         RETURNING note_uuid, kind, title, body, status, tags, data, created_at, updated_at",
    )
    .bind(session)
    .bind(title)
    .bind(tags)
    .fetch_one(f.handle.pool())
    .await?)
}

#[tokio::test]
async fn ended_acceptance_notes_retain_launch_authority_and_updated_order() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let project = seed_project(f.handle.pool(), "/tmp/fleet-a").await?;
    let run = format!("fleet-run-{}", Uuid::new_v4());
    seed_run(&f.handle, &run).await?;
    let first = Uuid::new_v4();
    let replacement = Uuid::new_v4();
    seed_session(f.handle.pool(), project, first).await?;
    seed_session(f.handle.pool(), project, replacement).await?;
    let launch = acceptance_launch(&f, &run, first).await?;
    acceptance_launch(&f, &run, replacement).await?;
    let tags = vec!["acceptance-obligation".to_owned()];
    let original = seed_legacy_obligation(&f, first, "original", &tags).await?;
    seed_legacy_obligation(&f, replacement, "replacement", &tags).await?;
    f.handle
        .launch()
        .record_launch_closed(launch, "replaced", Some(true))
        .await?;
    f.handle
        .launch()
        .set_session_goal(SetSessionIdentity {
            session_uuid: first,
            role: Some("worker"),
            display_name: None,
            goal: None,
            next_steps: None,
            instructions: None,
            set_by: "test",
        })
        .await?;
    let updated_at = sqlx::query_scalar::<_, chrono::DateTime<chrono::Utc>>(
        "UPDATE workspace.session_notes SET status = 'done', updated_at = NOW()
         WHERE note_uuid = $1 RETURNING updated_at",
    )
    .bind(original.note_uuid)
    .fetch_one(f.handle.pool())
    .await?;
    let rows = f.handle.fleet().fleet_note_rows(&run).await?;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].session_uuid, replacement);
    assert_eq!(rows[1].note_uuid, original.note_uuid);
    assert_eq!(rows[1].role.as_deref(), Some("acceptance"));
    assert_eq!(rows[1].seat.as_deref(), Some("acceptance"));
    assert_eq!(rows[1].body, original.body);
    assert_eq!(rows[1].tags, tags);
    assert_eq!(rows[1].updated_at, updated_at);
    assert_eq!(rows[1].created_at, original.created_at);
    f.db.cleanup().await
}

#[tokio::test]
async fn tagged_notes_are_project_scoped_and_do_not_inherit_unrelated_authority()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run = format!("fleet-run-{}", Uuid::new_v4());
    seed_run(&f.handle, &run).await?;
    let fleet = f
        .handle
        .fleet()
        .insert_fleet(&new_fleet("tagged", "/tmp/fleet-a"))
        .await?;
    seed_fleet_running(&f.handle, fleet.fleet_uuid, &run).await?;
    let tags = vec![
        "acceptance-obligation".to_owned(),
        format!("fleet:{}", fleet.fleet_uuid),
    ];
    let project = seed_project(f.handle.pool(), "/tmp/fleet-a").await?;
    let other_project = seed_project(f.handle.pool(), "/tmp/fleet-b").await?;
    let same_project = Uuid::new_v4();
    let different_project = Uuid::new_v4();
    seed_session(f.handle.pool(), project, same_project).await?;
    seed_session(f.handle.pool(), other_project, different_project).await?;
    let another_run = format!("fleet-run-{}", Uuid::new_v4());
    seed_run(&f.handle, &another_run).await?;
    acceptance_launch(&f, &another_run, same_project).await?;
    f.handle
        .launch()
        .set_session_goal(SetSessionIdentity {
            session_uuid: same_project,
            role: Some("acceptance"),
            display_name: None,
            goal: None,
            next_steps: None,
            instructions: None,
            set_by: "test",
        })
        .await?;
    let included = seed_legacy_obligation(&f, same_project, "recoverable", &tags).await?;
    seed_legacy_obligation(&f, different_project, "wrong project", &tags).await?;
    seed_legacy_obligation(&f, same_project, "wrong fleet", &["fleet:other".into()]).await?;
    let rows = f.handle.fleet().fleet_note_rows(&run).await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].note_uuid, included.note_uuid);
    assert_eq!(rows[0].session_uuid, same_project);
    assert_eq!(rows[0].role, None);
    assert_eq!(rows[0].seat, None);
    f.db.cleanup().await
}
