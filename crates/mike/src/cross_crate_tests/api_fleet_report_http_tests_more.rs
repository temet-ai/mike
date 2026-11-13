use crate::change_record::{ChangeKind, NewChangeEvent};
use crate::store::persistence::SENTINEL_PROJECT_PATH;
use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use chrono::Utc;
use reqwest::StatusCode;
use serde_json::{Value, json};

use super::{ACCEPTANCE, Harness, OTHER, RUN, WORKER, id, report_body};
use crate::store::persistence::session_notes::NewSessionNote;

impl Harness {
    async fn record(
        &self,
        kind: ChangeKind,
        detail: Value,
        sha: Option<&str>,
    ) -> anyhow::Result<()> {
        let project_id = self
            .persist
            .find_project_id_by_path(SENTINEL_PROJECT_PATH)
            .await?
            .ok_or_else(|| anyhow::anyhow!("the seeded sessions have no project"))?;
        let session_id = self
            .persist
            .resolve_session_db_id(&id(WORKER).to_string())
            .await?;
        let event = NewChangeEvent {
            project_id,
            checkout: self.def.repo.clone(),
            at: Utc::now(),
            kind,
            session_id: (kind != ChangeKind::Check).then_some(session_id).flatten(),
            run_id: Some(RUN.into()),
            task: Some("P1".into()),
            paths: vec!["crates/mike/src/store/persistence/improvement/mod.rs".into()],
            subject: kind.as_str().into(),
            detail,
            before_sha: None,
            after_sha: sha.map(str::to_string),
        };
        self.persist.fleet().insert_change_events(&[event]).await?;
        Ok(())
    }
}

#[tokio::test]
async fn with_the_record_on_a_report_says_whether_the_check_ran_after_the_seats_last_change()
-> anyhow::Result<()> {
    let Some(h) = Harness::boot().await? else {
        return Ok(());
    };
    h.persist
        .upsert_feature_toggle("change_record", true)
        .await?;
    let head = h.head.clone();
    h.committed(&head).await?;
    h.record(ChangeKind::Edit, json!({}), None).await?;
    h.record(
        ChangeKind::Check,
        json!({"commit": head, "passed": true}),
        Some(&head),
    )
    .await?;

    let (code, first) = h.post("report", report_body(WORKER, &head)).await?;
    assert_eq!(code, StatusCode::OK, "{first}");
    assert_eq!(first["check"], "passed", "{first}");
    assert_eq!(first["checked_after_last_change"]["ran"], true, "{first}");
    assert_eq!(
        first["checked_after_last_change"]["verdict"], "passed",
        "{first}"
    );
    let shown = first["acceptance_was_shown"].as_str().unwrap_or_default();
    assert!(
        shown.contains("The check ran after the seat's last change: yes, passed."),
        "{shown}"
    );

    h.record(ChangeKind::Edit, json!({}), None).await?;
    let (code, second) = h.post("report", report_body(WORKER, &head)).await?;
    assert_eq!(code, StatusCode::OK, "{second}");
    assert_eq!(
        second["checked_after_last_change"]["ran"], false,
        "{second}"
    );
    let shown = second["acceptance_was_shown"].as_str().unwrap_or_default();
    assert!(
        shown.contains("The check ran after the seat's last change: no."),
        "{shown}"
    );
    h.cleanup().await
}

#[tokio::test]
async fn cross_task_regression_is_required_before_original_pass() -> anyhow::Result<()> {
    let Some(h) = Harness::boot().await? else {
        return Ok(());
    };
    let body = json!({
        "session": id(ACCEPTANCE), "task": "P1", "verdict": "pass", "scope": "final",
        "reason": "P1 is correct; page regression belongs to P2",
        "regressions": [{"task": "P2", "criteria": ["C2"], "reason": "hint missing",
            "failing_tests": ["hint_snapshot"], "evidence": ["results/hint.diff"]}]
    });
    let mut invalid = body.clone();
    invalid["regressions"][0]["criteria"] = json!(["C1"]);
    let (code, _) = h.post("verdict", invalid).await?;
    assert_eq!(code, StatusCode::BAD_REQUEST);
    let (code, verdict) = h.post("verdict", body).await?;
    assert_eq!(code, StatusCode::OK, "{verdict}");
    let obligation = verdict["regression_obligations"][0]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("no regression obligation returned"))?;
    assert!(
        h.inbox(OTHER)
            .await?
            .iter()
            .any(|mail| mail.contains(obligation))
    );
    let fleet = h
        .persist
        .fleet()
        .find_fleet("report")
        .await?
        .ok_or_else(|| anyhow::anyhow!("no fleet"))?;
    let filter = crate::store::persistence::session_notes::SessionNoteFilter {
        kind: Some("task"),
        status: None,
        tag: Some("acceptance-obligation"),
        q: None,
        order: crate::store::persistence::session_notes::NoteOrder::NewestFirst,
        limit: 20,
        offset: 0,
    };
    let rows = h
        .persist
        .launch()
        .fetch_session_notes(id(ACCEPTANCE), filter)
        .await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].data["task"], "P2");
    assert_eq!(rows[0].data["required"], true);
    assert_eq!(rows[0].data["outcome"], "failed");
    assert!(
        rows[0]
            .tags
            .contains(&format!("fleet:{}", fleet.fleet_uuid))
    );
    h.cleanup().await
}

#[tokio::test]
async fn invalid_obligation_create_and_patch_roll_back() -> anyhow::Result<()> {
    let Some(h) = Harness::boot().await? else {
        return Ok(());
    };
    let fleet = h
        .persist
        .fleet()
        .find_fleet("report")
        .await?
        .ok_or_else(|| anyhow::anyhow!("no fleet"))?;
    let tags = vec![
        "acceptance-obligation".into(),
        format!("fleet:{}", fleet.fleet_uuid),
    ];
    let valid =
        json!({"obligation_id": "page", "task": "P2", "criteria": ["C2"], "outcome": "failed"});
    let invalid =
        json!({"obligation_id": "bad", "task": "P2", "criteria": ["C1"], "outcome": "failed"});
    let note = NewSessionNote {
        kind: Some("task"),
        title: "page regression",
        body: None,
        status: None,
        tags: Some(&tags),
        data: Some(&invalid),
    };
    assert!(matches!(
        h.persist
            .fleet()
            .create_session_note(id(ACCEPTANCE), note)
            .await,
        Err(crate::store::persistence::PersistError::InvalidAcceptanceObligation(_))
    ));
    let row = h
        .persist
        .fleet()
        .create_session_note(
            id(ACCEPTANCE),
            NewSessionNote {
                data: Some(&valid),
                ..note
            },
        )
        .await?;
    let result = h
        .persist
        .fleet()
        .update_session_note(
            row.note_uuid,
            crate::store::persistence::session_notes::SessionNoteUpdate {
                data: Some(&invalid),
                ..Default::default()
            },
        )
        .await;
    assert!(matches!(
        result,
        Err(crate::store::persistence::PersistError::InvalidAcceptanceObligation(_))
    ));
    let filter = crate::store::persistence::session_notes::SessionNoteFilter {
        kind: Some("task"),
        status: None,
        tag: Some("acceptance-obligation"),
        q: None,
        order: crate::store::persistence::session_notes::NoteOrder::NewestFirst,
        limit: 20,
        offset: 0,
    };
    let rows = h
        .persist
        .launch()
        .fetch_session_notes(id(ACCEPTANCE), filter)
        .await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].data, valid);
    h.cleanup().await
}
