use crate::store::persistence::fleet_repo::FleetPersist as _;
use serde_json::json;
use uuid::Uuid;

use super::fixture;
use super::seed::{seed_launch, seed_run, seed_session};
use crate::store::persistence::session_notes::NewSessionNote;
use crate::store::persistence::test_support::seed_project;

#[tokio::test]
async fn note_rows_are_decisions_tasks_and_what_asks_the_operator_oldest_first_with_the_seat()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let project_id = seed_project(pool, "/tmp/fleet-a").await?;
    let run_id = format!("fleet-run-{}", Uuid::new_v4());
    seed_run(&f.handle, &run_id).await?;

    let inside = Uuid::new_v4();
    let outside = Uuid::new_v4();
    seed_session(pool, project_id, inside).await?;
    seed_session(pool, project_id, outside).await?;
    seed_launch(&f.handle, &run_id, "acceptance", Some(inside), true).await?;

    let payload = json!({"criterion": "C1", "verdict": "PASS"});
    let note = |kind: &'static str, title: &'static str, status| NewSessionNote {
        kind: Some(kind),
        title,
        body: None,
        status,
        tags: None,
        data: None,
    };
    f.handle
        .fleet()
        .create_session_note(inside, note("task", "write the table", Some("open")))
        .await?;
    f.handle
        .fleet()
        .create_session_note(inside, note("finding", "a finding, not a decision", None))
        .await?;
    f.handle
        .fleet()
        .create_session_note(
            inside,
            NewSessionNote {
                data: Some(&payload),
                ..note("decision", "ship it", None)
            },
        )
        .await?;
    f.handle
        .fleet()
        .create_session_note(
            inside,
            note(
                "question",
                "which platform do we build for",
                Some("operator"),
            ),
        )
        .await?;
    f.handle
        .fleet()
        .create_session_note(outside, note("decision", "another run's decision", None))
        .await?;
    f.handle
        .fleet()
        .create_session_note(
            outside,
            note("question", "another run's question", Some("operator")),
        )
        .await?;

    let rows = f.handle.fleet().fleet_note_rows(&run_id).await?;
    let titles: Vec<_> = rows.iter().map(|r| r.title.as_str()).collect();
    assert_eq!(
        titles,
        vec![
            "write the table",
            "ship it",
            "which platform do we build for"
        ]
    );
    assert_eq!(rows[2].kind, "question");
    assert_eq!(rows[2].status.as_deref(), Some("operator"));
    assert_eq!(rows[0].kind, "task");
    assert_eq!(rows[0].status.as_deref(), Some("open"));
    assert_eq!(rows[0].seat.as_deref(), Some("acceptance"));
    assert_eq!(rows[1].kind, "decision");
    assert_eq!(rows[1].status, None);
    assert_eq!(rows[1].data, payload);
    assert!(rows[0].created_at <= rows[1].created_at);

    assert!(
        f.handle
            .fleet()
            .fleet_note_rows("run-with-no-launches")
            .await?
            .is_empty()
    );
    f.db.cleanup().await
}
