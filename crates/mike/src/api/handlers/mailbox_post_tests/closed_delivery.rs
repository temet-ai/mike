use axum::response::IntoResponse;

use super::*;
use crate::api::handlers::{
    PostThreadBody, PostThreadMessageBody, post_thread, post_thread_message,
};
use crate::store::persistence::mailbox::ContextItem;
use crate::store::persistence::workspace::RegisterWorkspaceSession;

fn context() -> Vec<ContextItem> {
    vec![ContextItem {
        kind: "file".into(),
        value: "src/owned.rs".into(),
        label: None,
    }]
}

async fn assert_closed_audits(f: &Fleet, recipients: &[Uuid]) -> anyhow::Result<()> {
    let targets: Vec<String> = [f.lead.session, f.t1.session, f.t2.session]
        .iter()
        .map(Uuid::to_string)
        .collect();
    let found = sqlx::query!(
        "SELECT sender.session_uuid AS \"sender_uuid?\", failed.attempted_to, failed.reason
           FROM mailbox.failed_sends failed
           LEFT JOIN sessions.sessions sender ON sender.id = failed.attempted_by_session_id
          WHERE failed.attempted_to = ANY($1) OR sender.session_uuid = $2",
        &targets,
        f.lead.session,
    )
    .fetch_all(f.env.persist.pool())
    .await?;
    let mut rows: Vec<(Option<Uuid>, String, String)> = found
        .into_iter()
        .map(|row| (row.sender_uuid, row.attempted_to, row.reason))
        .collect();
    let mut expected: Vec<_> = recipients
        .iter()
        .map(|to| {
            (
                Some(f.lead.session),
                to.to_string(),
                "recipient_ended".to_owned(),
            )
        })
        .collect();
    rows.sort();
    expected.sort();
    assert_eq!(rows, expected);
    Ok(())
}

fn assert_failed_recipients(body: &Value, recipients: &[Uuid]) {
    let mut failed = body["failed"].as_array().cloned().unwrap_or_default();
    let mut expected: Vec<_> = recipients
        .iter()
        .map(|to| json!({"to_session_uuid": to, "reason": "recipient_closed"}))
        .collect();
    let by_recipient = |row: &Value| row["to_session_uuid"].to_string();
    failed.sort_by_key(by_recipient);
    expected.sort_by_key(by_recipient);
    assert_eq!(failed, expected);
}

#[tokio::test]
async fn closed_thread_notify_reports_failure_and_retains_post_body() -> anyhow::Result<()> {
    let Some(f) = fleet().await? else {
        return Ok(());
    };
    f.env.close(&f.t2).await?;
    let body = PostThreadBody {
        subject: "closed notify".into(),
        body: Some("keep this thread body".into()),
        from_session_id: Some(f.lead.session.to_string()),
        project_path: None,
        tags: Vec::new(),
        notify: vec![f.t2.session.to_string()],
    };
    let result = post_thread(State(f.env.state.base.clone()), Json(body)).await;
    let (status, body) = read(result.map(IntoResponse::into_response)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["deliveries"], json!([]));
    assert_failed_recipients(&body, &[f.t2.session]);
    assert_closed_audits(&f, &[f.t2.session]).await?;
    assert_eq!(f.env.inbox_len(f.t2.session).await?, 0);
    let id = Uuid::parse_str(
        body["thread_uuid"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("thread missing"))?,
    )?;
    let thread = f
        .env
        .persist
        .launch()
        .fetch_thread_core_by_uuid(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("created thread missing"))?;
    let messages = f
        .env
        .persist
        .launch()
        .fetch_thread_messages(thread.id)
        .await?;
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].body, "keep this thread body");
    f.env.cleanup().await
}

#[tokio::test]
async fn threaded_reply_counts_only_open_recipients_with_context() -> anyhow::Result<()> {
    let Some(f) = fleet().await? else {
        return Ok(());
    };
    f.env.close(&f.t2).await?;
    let thread = f
        .env
        .persist
        .launch()
        .create_thread(Uuid::new_v4(), "mixed reply", Some(f.env.project), None)
        .await?;
    let body = PostThreadMessageBody {
        from_session_id: Some(f.lead.session.to_string()),
        body: "reply with context".into(),
        to: Some(vec![f.t1.session.to_string(), f.t2.session.to_string()]),
        in_reply_to: None,
        context: context(),
    };
    let result = post_thread_message(
        State(f.env.state.base.clone()),
        Path(thread.thread_uuid),
        Json(body),
    )
    .await;
    let (status, body) = read(result.map(IntoResponse::into_response)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["deliveries"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        body["deliveries"][0]["to_session_uuid"],
        f.t1.session.to_string()
    );
    assert_failed_recipients(&body, &[f.t2.session]);
    assert_closed_audits(&f, &[f.t2.session]).await?;
    assert_eq!(f.env.inbox_len(f.t1.session).await?, 1);
    assert_eq!(f.env.inbox_len(f.t2.session).await?, 0);
    f.env.cleanup().await
}

async fn register_role(env: &Env, session: Uuid, role: &str) -> anyhow::Result<()> {
    env.persist
        .register_workspace_session(RegisterWorkspaceSession {
            session_uuid: session,
            project_path: None,
            cwd: CHECKOUT,
            worktree: None,
            registered_via: "passive",
            intent: None,
            role: Some(role),
            name: None,
            instructions: None,
            pid: None,
        })
        .await?;
    Ok(())
}

#[tokio::test]
async fn role_fanout_rejects_closed_launch_despite_live_registration() -> anyhow::Result<()> {
    let Some(f) = fleet().await? else {
        return Ok(());
    };
    f.env.close(&f.t2).await?;
    for session in [f.t1.session, f.t2.session] {
        register_role(&f.env, session, "worker").await?;
    }
    let (status, body) = f
        .env
        .send(Some(f.lead.session), "role:worker", Some("mixed fanout"))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["deliveries"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        body["deliveries"][0]["to_session_uuid"],
        f.t1.session.to_string()
    );
    assert_failed_recipients(&body, &[f.t2.session]);
    assert_closed_audits(&f, &[f.t2.session]).await?;
    assert_eq!(f.env.inbox_len(f.t2.session).await?, 0);
    f.env.close(&f.t1).await?;
    register_role(&f.env, f.t1.session, "worker").await?;
    let (status, body) = f
        .env
        .send(Some(f.lead.session), "role:worker", Some("closed fanout"))
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["deliveries"], json!([]));
    assert_failed_recipients(&body, &[f.t1.session, f.t2.session]);
    assert_closed_audits(&f, &[f.t2.session, f.t1.session, f.t2.session]).await?;
    assert_eq!(f.env.inbox_len(f.t1.session).await?, 1);
    assert_eq!(f.env.inbox_len(f.t2.session).await?, 0);
    f.env.cleanup().await
}

#[tokio::test]
async fn direct_closed_send_with_context_is_refused_before_queueing() -> anyhow::Result<()> {
    let Some(f) = fleet().await? else {
        return Ok(());
    };
    f.env.close(&f.t2).await?;
    for (attempt, subject) in [None, Some("threaded direct".into())]
        .into_iter()
        .enumerate()
    {
        let body = PostSessionMessageBody {
            from_session_id: Some(f.lead.session.to_string()),
            message: "must not queue".into(),
            thread_id: None,
            in_reply_to: None,
            subject,
            tags: None,
            context: context(),
        };
        let (status, body) = read(
            post_session_message(
                State(f.env.state.clone()),
                Path(f.t2.session.to_string()),
                Json(body),
            )
            .await,
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["error"], "recipient_closed");
        assert!(
            body["message"]
                .as_str()
                .is_some_and(|text| text.contains(&f.t2.session.to_string()))
        );
        assert_closed_audits(&f, &vec![f.t2.session; attempt + 1]).await?;
    }
    assert_eq!(f.env.inbox_len(f.t2.session).await?, 0);
    f.env.cleanup().await
}
