use chrono::Utc;
use serde_json::json;
use tokio_util::task::TaskTracker;
use uuid::Uuid;

use super::{events_in, fixture, repo, seed_session};
use crate::change_record::ChangeKind;
use crate::change_record::writers::{
    CheckFacts, MailFacts, NoteFacts, record_check, record_mail, record_note, spawn_mail_event,
    spawn_note_event,
};
use crate::store::persistence::mailbox::ContextItem;

fn note(data: serde_json::Value) -> NoteFacts {
    NoteFacts {
        note_uuid: Uuid::new_v4(),
        note_kind: "finding".to_string(),
        title: "the parser trap".to_string(),
        body: "it breaks in src/lib.rs when empty".to_string(),
        status: None,
        data,
        at: Utc::now(),
    }
}

fn mail(from: Option<Uuid>, to: Uuid, context: Vec<ContextItem>) -> MailFacts {
    MailFacts {
        message_uuid: Uuid::new_v4(),
        from,
        to,
        body: "\n  look at the failing line\nsecond line".to_string(),
        context,
        at: Utc::now(),
    }
}

fn item(kind: &str, value: &str) -> ContextItem {
    ContextItem {
        kind: kind.to_string(),
        value: value.to_string(),
        label: None,
    }
}

#[tokio::test]
async fn record_note_data_paths_come_first_then_paths_named_in_the_text() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let repo = repo()?;
    let seeded = seed_session(&f.handle, "/p/notes", repo.path()).await?;
    let facts =
        note(json!({ "paths": ["README.md", format!("{}/src/other.rs", repo.checkout())] }));
    let id = record_note(&f.handle, seeded.session_uuid, &facts).await?;
    assert!(id.is_some());
    let events = events_in(&f.handle, &repo.checkout()).await?;
    assert_eq!(events.len(), 1);
    let event = &events[0];
    assert_eq!(event.kind, ChangeKind::Note);
    assert_eq!(event.session_id, Some(seeded.session_id));
    assert_eq!(event.subject, "the parser trap");
    assert_eq!(event.paths, vec!["README.md", "src/other.rs", "src/lib.rs"]);
    assert_eq!(event.detail["note_uuid"], json!(facts.note_uuid));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn record_note_with_a_verdict_in_its_data_is_a_verdict_event() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let repo = repo()?;
    let seeded = seed_session(&f.handle, "/p/verdict", repo.path()).await?;
    let facts = note(json!({ "verdict": "pass", "criteria": ["C1"] }));
    record_note(&f.handle, seeded.session_uuid, &facts).await?;
    let events = events_in(&f.handle, &repo.checkout()).await?;
    assert_eq!(events[0].kind, ChangeKind::Verdict);
    assert_eq!(events[0].detail["verdict"], json!("pass"));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn record_note_session_outside_a_repository_writes_nothing() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let dir = super::plain_dir()?;
    let seeded = seed_session(&f.handle, "/p/plain", dir.path()).await?;
    let id = record_note(&f.handle, seeded.session_uuid, &note(json!({}))).await?;
    assert_eq!(id, None);
    assert_eq!(
        record_note(&f.handle, Uuid::new_v4(), &note(json!({}))).await?,
        None
    );
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn record_mail_context_file_and_commit_name_the_paths() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let repo = repo()?;
    let sender = seed_session(&f.handle, "/p/mail", repo.path()).await?;
    let receiver = seed_session(&f.handle, "/p/mail", repo.path()).await?;
    let context = vec![
        item("file", "docs/x.md"),
        item("commit", &repo.first_commit),
    ];
    let facts = mail(Some(sender.session_uuid), receiver.session_uuid, context);
    record_mail(&f.handle, &facts).await?;
    let events = events_in(&f.handle, &repo.checkout()).await?;
    assert_eq!(events.len(), 1);
    let event = &events[0];
    assert_eq!(event.kind, ChangeKind::Mail);
    assert_eq!(event.session_id, Some(sender.session_id));
    assert_eq!(event.subject, "look at the failing line");
    assert_eq!(event.paths[0], "docs/x.md");
    assert!(event.paths.contains(&"src/lib.rs".to_string()));
    assert_eq!(event.detail["to"], json!(receiver.session_uuid));
    assert_eq!(event.detail["message_uuid"], json!(facts.message_uuid));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn record_mail_without_a_sender_has_no_session() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let repo = repo()?;
    let receiver = seed_session(&f.handle, "/p/anon", repo.path()).await?;
    record_mail(&f.handle, &mail(None, receiver.session_uuid, vec![])).await?;
    let events = events_in(&f.handle, &repo.checkout()).await?;
    assert_eq!(events[0].session_id, None);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn record_check_failing_files_become_the_paths() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let repo = repo()?;
    let seeded = seed_session(&f.handle, "/p/check", repo.path()).await?;
    let facts = CheckFacts {
        project_id: seeded.project_id,
        checkout: repo.checkout(),
        run_id: Some("run-1".to_string()),
        label: "check".to_string(),
        command: "make check".to_string(),
        commit: repo.first_commit.clone(),
        passed: false,
        source_verified: true,
        failing_paths: vec!["src/lib.rs".to_string(), "src/lib.rs".to_string()],
        at: Utc::now(),
    };
    record_check(&f.handle, facts).await?;
    let events = events_in(&f.handle, &repo.checkout()).await?;
    assert_eq!(events[0].kind, ChangeKind::Check);
    assert_eq!(events[0].paths, vec!["src/lib.rs"]);
    assert_eq!(events[0].run_id.as_deref(), Some("run-1"));
    assert_eq!(events[0].detail["passed"], json!(false));
    assert!(events[0].subject.starts_with("check failed at "));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn spawn_events_with_the_switch_off_write_nothing_and_with_it_on_write_one()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let repo = repo()?;
    let seeded = seed_session(&f.handle, "/p/spawn", repo.path()).await?;
    let tracker = TaskTracker::new();
    spawn_note_event(
        &tracker,
        f.handle.clone(),
        false,
        seeded.session_uuid,
        note(json!({})),
    );
    spawn_mail_event(
        &tracker,
        f.handle.clone(),
        false,
        mail(Some(seeded.session_uuid), seeded.session_uuid, vec![]),
    );
    tracker.close();
    tracker.wait().await;
    assert!(events_in(&f.handle, &repo.checkout()).await?.is_empty());

    let tracker = TaskTracker::new();
    spawn_note_event(
        &tracker,
        f.handle.clone(),
        true,
        seeded.session_uuid,
        note(json!({})),
    );
    spawn_mail_event(
        &tracker,
        f.handle.clone(),
        true,
        mail(Some(seeded.session_uuid), seeded.session_uuid, vec![]),
    );
    tracker.close();
    tracker.wait().await;
    let kinds: Vec<ChangeKind> = events_in(&f.handle, &repo.checkout())
        .await?
        .iter()
        .map(|e| e.kind)
        .collect();
    assert_eq!(kinds.len(), 2);
    assert!(kinds.contains(&ChangeKind::Note) && kinds.contains(&ChangeKind::Mail));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn record_functions_report_a_database_error_when_the_pool_is_closed() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let repo = repo()?;
    let seeded = seed_session(&f.handle, "/p/closed", repo.path()).await?;
    f.handle.pool().close().await;
    assert!(
        record_note(&f.handle, seeded.session_uuid, &note(json!({})))
            .await
            .is_err()
    );
    assert!(
        record_mail(&f.handle, &mail(None, seeded.session_uuid, vec![]))
            .await
            .is_err()
    );
    let facts = CheckFacts {
        project_id: seeded.project_id,
        checkout: repo.checkout(),
        run_id: None,
        label: "gate".to_string(),
        command: "x".to_string(),
        commit: "abc".to_string(),
        passed: true,
        source_verified: true,
        failing_paths: vec![],
        at: Utc::now(),
    };
    assert!(record_check(&f.handle, facts).await.is_err());
    Ok(())
}
