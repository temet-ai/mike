use super::test_support::{
    empty_store, mk_finding, noop_push_deps, sample_request, user_turn_body,
};
use super::*;
use crate::store::models::RequestId;
use serde_json::{Value, json};

#[tokio::test]
async fn disabled_short_circuits_with_no_store_touch() -> anyhow::Result<()> {
    let store = empty_store();
    let cfg = RemindersConfig {
        enabled: false,
        ..Default::default()
    };
    let body = user_turn_body()?;
    let outcome = apply_reminders(
        &body,
        "session-A",
        &store,
        &cfg,
        RequestId::next(),
        &noop_push_deps(),
    )
    .await;
    match outcome {
        RemindersOutcome::Unchanged => Ok(()),
        other => Err(anyhow::anyhow!(
            "expected Unchanged when disabled — got {other:?}"
        )),
    }
}

#[tokio::test]
async fn no_prior_request_returns_unchanged() -> anyhow::Result<()> {
    let store = empty_store();
    let cfg = RemindersConfig {
        enabled: true,
        ..Default::default()
    };
    let body = user_turn_body()?;
    let outcome = apply_reminders(
        &body,
        "session-no-prior",
        &store,
        &cfg,
        RequestId::next(),
        &noop_push_deps(),
    )
    .await;
    match outcome {
        RemindersOutcome::Unchanged => Ok(()),
        other => Err(anyhow::anyhow!(
            "expected Unchanged when no prior request exists — got {other:?}"
        )),
    }
}

#[tokio::test]
async fn prior_request_with_zero_findings_returns_unchanged() -> anyhow::Result<()> {
    let store = empty_store();
    {
        let mut guard = store.write().await;
        let req = sample_request(RequestId::from_raw(1), "session-zero-findings");
        guard.insert(req);
    }
    let cfg = RemindersConfig {
        enabled: true,
        ..Default::default()
    };
    let body = user_turn_body()?;
    let outcome = apply_reminders(
        &body,
        "session-zero-findings",
        &store,
        &cfg,
        RequestId::next(),
        &noop_push_deps(),
    )
    .await;
    match outcome {
        RemindersOutcome::Unchanged => Ok(()),
        other => Err(anyhow::anyhow!(
            "expected Unchanged when prior request has zero findings — got {other:?}"
        )),
    }
}

#[tokio::test]
async fn prior_request_with_findings_appends_reminder() -> anyhow::Result<()> {
    let store = empty_store();
    let prev_id_u64 = 42_u64;
    {
        let mut guard = store.write().await;
        let mut req = sample_request(RequestId::from_raw(prev_id_u64), "session-found");
        req.diagnostics = vec![
            mk_finding("comprehensive solution", "no MVP without ask", 0),
            mk_finding("for now", "There is no 'later'", 100),
        ];
        guard.insert(req);
    }
    let cfg = RemindersConfig {
        enabled: true,
        ..Default::default()
    };
    let body = user_turn_body()?;
    let outcome = apply_reminders(
        &body,
        "session-found",
        &store,
        &cfg,
        RequestId::next(),
        &noop_push_deps(),
    )
    .await;
    match outcome {
        RemindersOutcome::Appended {
            body,
            reminder_text,
            source_finding_ids,
            source_request_id,
        } => {
            assert_eq!(source_request_id, prev_id_u64);
            assert_eq!(source_finding_ids, vec![0_i64, 100_i64]);
            assert!(reminder_text.contains("<system-reminder source=\"proxy-reminders\">"));
            assert!(reminder_text.contains("comprehensive solution"));
            assert!(reminder_text.contains("for now"));

            let new_body: Value = serde_json::from_slice(&body)?;
            let last_content = new_body["messages"][0]["content"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("content must be array"))?;
            assert_eq!(last_content.len(), 2);
            assert_eq!(last_content[0]["text"], "follow-up");
            assert_eq!(last_content[1]["type"], "text");
            assert_eq!(last_content[1]["text"], reminder_text);
            Ok(())
        }
        other => Err(anyhow::anyhow!(
            "expected Appended when prior request has findings — got {other:?}"
        )),
    }
}

#[tokio::test]
async fn assistant_last_turn_returns_unchanged_even_with_findings() -> anyhow::Result<()> {
    let store = empty_store();
    {
        let mut guard = store.write().await;
        let mut req = sample_request(RequestId::from_raw(7), "session-assistant-last");
        req.diagnostics = vec![mk_finding("comprehensive", "X", 0)];
        guard.insert(req);
    }
    let cfg = RemindersConfig {
        enabled: true,
        ..Default::default()
    };
    let body = serde_json::to_vec(&json!({
        "model": "claude",
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "q"}]},
            {"role": "assistant", "content": [{"type": "text", "text": "a"}]},
        ],
    }))?;
    let outcome = apply_reminders(
        &body,
        "session-assistant-last",
        &store,
        &cfg,
        RequestId::next(),
        &noop_push_deps(),
    )
    .await;
    match outcome {
        RemindersOutcome::Unchanged => Ok(()),
        other => Err(anyhow::anyhow!(
            "expected Unchanged when last turn is assistant — got {other:?}"
        )),
    }
}

#[tokio::test]
async fn finds_only_session_scoped_prior_request() -> anyhow::Result<()> {
    let store = empty_store();
    {
        let mut guard = store.write().await;
        let mut other = sample_request(RequestId::from_raw(99), "session-other");
        other.diagnostics = vec![mk_finding("comprehensive", "X", 0)];
        guard.insert(other);
    }
    let cfg = RemindersConfig {
        enabled: true,
        ..Default::default()
    };
    let body = user_turn_body()?;
    let outcome = apply_reminders(
        &body,
        "session-target",
        &store,
        &cfg,
        RequestId::next(),
        &noop_push_deps(),
    )
    .await;
    match outcome {
        RemindersOutcome::Unchanged => Ok(()),
        other => Err(anyhow::anyhow!(
            "another session's findings must not trigger a reminder — got {other:?}"
        )),
    }
}
