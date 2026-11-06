use super::*;
use crate::recall_injector::answer_recall;
use serde_json::{Value, json};

fn noop_push_deps() -> PushDeps {
    PushDeps {
        persist: None,
        remote_control_streams: crate::proxy::remote_control::ReplyStreams::default(),
        gate: std::sync::Arc::new(crate::injection::InjectionGate::default()),
        store: std::sync::Arc::new(tokio::sync::RwLock::new(crate::store::RequestStore::new(
            crate::store::retention::RetentionPolicy::default(),
        ))),
    }
}

#[test]
fn config_default_is_disabled() {
    let cfg = RecallInjectorConfig::default();
    assert!(!cfg.enabled);
    assert_eq!(cfg.max_lessons_per_topic, 5);
}

#[test]
fn collect_assistant_text_string_content() {
    let v = json!({
        "messages": [
            {"role": "user", "content": "hello"},
            {"role": "assistant", "content": "world <mem:recall topic=\"x\"/>"},
        ]
    });
    let text = collect_assistant_text(&v);
    assert!(text.contains("<mem:recall topic=\"x\"/>"));
    assert!(!text.contains("hello"));
}

#[test]
fn collect_assistant_text_array_content() {
    let v = json!({
        "messages": [
            {
                "role": "assistant",
                "content": [
                    {"type": "text", "text": "block text <mem:recall topic=\"y\"/>"},
                    {"type": "tool_use", "id": "t1", "name": "Bash", "input": {}}
                ]
            }
        ]
    });
    let text = collect_assistant_text(&v);
    assert!(text.contains("block text <mem:recall topic=\"y\"/>"));
}

#[test]
fn collect_assistant_text_no_messages_returns_empty() {
    let v = json!({"model": "claude-3-5-sonnet-20241022"});
    assert_eq!(collect_assistant_text(&v), "");
}

#[tokio::test]
async fn apply_recall_injection_unchanged_when_disabled() -> anyhow::Result<()> {
    let body = json!({
        "messages": [{"role": "assistant", "content": "<mem:recall topic=\"x\"/>"},
                     {"role": "user", "content": [{"type": "text", "text": "hi"}]}]
    });
    let raw = serde_json::to_vec(&body)?;
    let cfg = RecallInjectorConfig {
        enabled: false,
        max_lessons_per_topic: 5,
    };
    let outcome = apply_recall_injection(RecallInjectionRequest {
        raw: &raw,
        project_path: "/test",
        persist: &None,
        cfg: &cfg,
        request_id: RequestId::next(),
        session_id: "session-recall-test",
        push_deps: &noop_push_deps(),
    })
    .await;
    assert!(matches!(
        outcome,
        RecallOutcome::Declined("recall_disabled")
    ));
    Ok(())
}

#[tokio::test]
async fn apply_recall_injection_unchanged_when_no_tags() -> anyhow::Result<()> {
    let body = json!({
        "messages": [{"role": "assistant", "content": "no tags here"},
                     {"role": "user", "content": [{"type": "text", "text": "hi"}]}]
    });
    let raw = serde_json::to_vec(&body)?;
    let cfg = RecallInjectorConfig {
        enabled: true,
        max_lessons_per_topic: 5,
    };
    let outcome = apply_recall_injection(RecallInjectionRequest {
        raw: &raw,
        project_path: "/test",
        persist: &None,
        cfg: &cfg,
        request_id: RequestId::next(),
        session_id: "session-recall-test",
        push_deps: &noop_push_deps(),
    })
    .await;
    assert!(matches!(outcome, RecallOutcome::Declined("recall_no_tags")));
    Ok(())
}

#[tokio::test]
async fn apply_recall_injection_r1_guard_skips_thinking_blocks() -> anyhow::Result<()> {
    let body = json!({
        "messages": [
            {
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "..."},
                    {"type": "text", "text": "<mem:recall topic=\"x\"/>"}
                ]
            },
            {"role": "user", "content": [{"type": "text", "text": "ok"}]}
        ]
    });
    let raw = serde_json::to_vec(&body)?;
    let cfg = RecallInjectorConfig {
        enabled: true,
        max_lessons_per_topic: 5,
    };
    let outcome = apply_recall_injection(RecallInjectionRequest {
        raw: &raw,
        project_path: "/test",
        persist: &None,
        cfg: &cfg,
        request_id: RequestId::next(),
        session_id: "session-recall-test",
        push_deps: &noop_push_deps(),
    })
    .await;
    assert!(matches!(
        outcome,
        RecallOutcome::Declined("recall_thinking_guard")
    ));
    Ok(())
}

#[tokio::test]
async fn apply_recall_injection_unchanged_when_no_persist() -> anyhow::Result<()> {
    let body = json!({
        "messages": [
            {"role": "assistant", "content": "<mem:recall topic=\"x\"/>"},
            {"role": "user", "content": [{"type": "text", "text": "hi"}]}
        ]
    });
    let raw = serde_json::to_vec(&body)?;
    let cfg = RecallInjectorConfig {
        enabled: true,
        max_lessons_per_topic: 5,
    };
    let outcome = apply_recall_injection(RecallInjectionRequest {
        raw: &raw,
        project_path: "/test/project",
        persist: &None,
        cfg: &cfg,
        request_id: RequestId::next(),
        session_id: "session-recall-test",
        push_deps: &noop_push_deps(),
    })
    .await;
    assert!(matches!(
        outcome,
        RecallOutcome::Declined("recall_no_persist")
    ));
    Ok(())
}

#[tokio::test]
async fn recall_route_text_is_the_text_the_injector_appends() -> anyhow::Result<()> {
    let Some(db) = crate::store::persistence::test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = crate::store::persistence::PersistHandle::connect(&db.database_url).await?;
    let project_path = format!("/tmp/recall-route-text-{}", uuid::Uuid::new_v4());
    let project_id =
        crate::store::persistence::test_support::seed_project(handle.pool(), &project_path).await?;
    handle
        .create_lesson_with_topic(crate::store::persistence::lessons::NewLessonWrite {
            project_id,
            kind: "convention",
            text: "use the staging bucket for deploy checks",
            topic: Some("deploy"),
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?;
    let topics = vec!["deploy".to_owned()];
    let cfg = RecallInjectorConfig {
        enabled: true,
        max_lessons_per_topic: 5,
    };
    let answer = answer_recall(&handle, &project_path, &topics, &cfg).await;
    let Some(route_text) = answer.text.clone() else {
        anyhow::bail!("route answered no text: {answer:?}");
    };
    assert_eq!(answer.lesson_ids.len(), 1);

    let raw = serde_json::to_vec(&json!({
        "messages": [
            {"role": "user", "content": "check the deploy"},
            {"role": "assistant", "content": "<mem:recall topic=\"deploy\"/>"},
            {"role": "user", "content": [{"type": "text", "text": "ok"}]}
        ]
    }))?;
    let persist = Some(handle);
    let outcome = apply_recall_injection(RecallInjectionRequest {
        raw: &raw,
        project_path: &project_path,
        persist: &persist,
        cfg: &cfg,
        request_id: RequestId::next(),
        session_id: &uuid::Uuid::new_v4().to_string(),
        push_deps: &noop_push_deps(),
    })
    .await;
    let RecallOutcome::Injected { new_body, .. } = outcome else {
        anyhow::bail!("injector declined: {outcome:?}");
    };
    let injected = serde_json::from_slice::<Value>(&new_body)?.to_string();
    let quoted = serde_json::to_string(&route_text)?;
    let Some(escaped) = quoted
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
    else {
        anyhow::bail!("route text is not a JSON string: {quoted}");
    };
    assert!(injected.contains(escaped));
    db.cleanup().await?;
    Ok(())
}
