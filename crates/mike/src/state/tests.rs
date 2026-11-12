use std::sync::Arc;
use std::sync::atomic::Ordering;

use tokio::sync::broadcast;

use clap::Parser;

use super::AppState;
use crate::config::ProxyConfig;
use crate::config::load::CliOverrides;

fn parse(args: &[&str]) -> anyhow::Result<ProxyConfig> {
    let overrides = CliOverrides::try_parse_from(args)?;
    crate::config::load::load_with(overrides, &crate::environment::MapEnvironment::isolated())
}

#[tokio::test]
async fn test_app_state_new() -> anyhow::Result<()> {
    let config = parse(&["mike", "--persist-enabled", "0"])?;
    let state = AppState::new(config).await?;
    assert_eq!(state.config.max_requests, 1000);
    assert_eq!(state.config.proxy_listen_addr, "127.0.0.1:9988");
    assert_eq!(state.config.inspect_listen_addr, "127.0.0.1:9989");
    assert_eq!(state.retention.max_entries, 1000);
    assert!(
        state.persist().is_none(),
        "persist must be None when --persist-enabled=0"
    );
    Ok(())
}

#[tokio::test]
async fn session_toggles_freeze_at_first_sight_new_sessions_see_flips() -> anyhow::Result<()> {
    use crate::feature_toggles::FeatureToggles;
    use crate::settings::SettingsOverlay;

    let config = parse(&["mike", "--persist-enabled", "0"])?;
    let state = AppState::new(config).await?;

    let a_first = state.resolve_session_toggles("session-A");
    assert!(!a_first.reminders, "session A must snapshot reminders=OFF");
    assert!(!a_first.recall_injector);
    assert!(!a_first.weasel_intercept);
    assert!(
        !a_first.reminders_inject_lessons,
        "reminders_inject_lessons must default OFF"
    );
    assert_eq!(
        a_first.reminders_max_lessons_to_inject, 5,
        "default max_lessons_to_inject"
    );
    assert_eq!(
        a_first.recall_injector_max_lessons_per_topic, 3,
        "default max_lessons_per_topic"
    );

    let flipped = FeatureToggles {
        reminders: true,
        ..FeatureToggles::default()
    };
    state.feature_toggle_overlay.store(Arc::new(flipped));

    let new_settings = SettingsOverlay {
        reminders_max_lessons_to_inject: 99,
        ..SettingsOverlay::default()
    };
    state.settings_overlay.store(Arc::new(new_settings));

    let a_second = state.resolve_session_toggles("session-A");
    assert!(
        !a_second.reminders,
        "in-flight session A must keep frozen reminders=OFF after a mid-session flip"
    );
    assert_eq!(
        a_second.reminders_max_lessons_to_inject, 5,
        "in-flight session A must keep the frozen max_lessons_to_inject=5, not 99"
    );
    assert!(
        Arc::ptr_eq(&a_first, &a_second),
        "re-resolving an existing session must return the same cached snapshot, not re-evaluate"
    );

    let b_first = state.resolve_session_toggles("session-B");
    assert!(
        b_first.reminders,
        "new session B must observe the flipped reminders=ON"
    );
    assert_eq!(
        b_first.reminders_max_lessons_to_inject, 99,
        "new session B must observe the updated max_lessons_to_inject=99"
    );

    Ok(())
}

#[tokio::test]
async fn app_state_new_errors_without_database_url_when_enabled() -> anyhow::Result<()> {
    let config = parse(&["mike", "--persist-enabled", "1"])?;
    match AppState::new(config).await {
        Err(e) => {
            let msg = format!("{e}");
            assert!(
                msg.contains("MIKE_DATABASE_URL")
                    || msg.contains("database_url")
                    || msg.contains("persistence is enabled"),
                "error message must mention database URL: {msg}"
            );
            Ok(())
        }
        Ok(_) => anyhow::bail!("expected error, got AppState"),
    }
}

#[tokio::test]
async fn app_state_new_errors_on_unreachable_database() -> anyhow::Result<()> {
    let tmp = tempfile::TempDir::new()?;
    let data_dir = tmp.path().to_string_lossy().to_string();
    let mut config = parse(&[
        "mike",
        "--persist-enabled",
        "1",
        "--database-url",
        "postgres://nobody:nobody@127.0.0.1:1/does_not_exist",
        "--data-dir",
        &data_dir,
    ])?;
    config.db_connect_timeout_ms = 200;
    match AppState::new(config).await {
        Err(_) => Ok(()),
        Ok(_) => anyhow::bail!("expected error when DB is unreachable"),
    }
}

#[tokio::test]
async fn app_state_and_store_share_same_analyzer_handle() -> anyhow::Result<()> {
    let config = parse(&["mike", "--persist-enabled", "0"])?;
    let state = AppState::new(config).await?;

    let app_handle = state.analyzer_registry().clone();
    let store = state.store();
    let store_guard = store.read().await;
    let (store_handle, _) = store_guard
        .analysis_wired()
        .ok_or_else(|| anyhow::anyhow!("analysis should be wired by AppState::new"))?;

    assert!(
        Arc::ptr_eq(&app_handle, store_handle),
        "AppState and RequestStore must share the SAME Arc<ArcSwap<AnalyzerRegistry>>"
    );
    Ok(())
}

#[tokio::test]
async fn test_request_counter() -> anyhow::Result<()> {
    let config = parse(&["mike", "--persist-enabled", "0"])?;
    let state = AppState::new(config).await?;
    assert_eq!(state.request_count.load(Ordering::Relaxed), 0);
    assert_eq!(state.inc_request_count(), 1);
    assert_eq!(state.inc_request_count(), 2);
    assert_eq!(state.request_count.load(Ordering::Relaxed), 2);
    Ok(())
}

#[tokio::test]
async fn capturable_traffic_has_its_own_clock() -> anyhow::Result<()> {
    let config = parse(&["mike", "--persist-enabled", "0"])?;
    let state = AppState::new(config).await?;
    state.inc_request_count();
    assert!(
        state.last_capturable_request_at().is_none(),
        "a request the proxy accepted is not yet one the capture path must write"
    );
    let before = chrono::Utc::now();
    state.note_capturable_request();
    let stamped = state
        .last_capturable_request_at()
        .ok_or_else(|| anyhow::anyhow!("capturable clock must be set after a capture"))?;
    assert!(stamped >= before - chrono::Duration::seconds(1));
    Ok(())
}

#[tokio::test]
async fn inc_request_count_stamps_last_request_at() -> anyhow::Result<()> {
    let config = parse(&["mike", "--persist-enabled", "0"])?;
    let state = AppState::new(config).await?;
    assert!(
        state.last_request_at().is_none(),
        "a fresh process has handled no traffic yet"
    );

    let before = chrono::Utc::now();
    state.inc_request_count();
    let after = chrono::Utc::now();

    let stamped = state
        .last_request_at()
        .ok_or_else(|| anyhow::anyhow!("last_request_at must be Some after a request"))?;
    assert!(
        stamped >= before - chrono::Duration::seconds(1) && stamped <= after,
        "stamped timestamp {stamped} must fall within the call window [{before}, {after}]"
    );
    Ok(())
}

#[tokio::test]
async fn test_event_publish_subscribe() -> anyhow::Result<()> {
    use crate::store::models::{ProxyEvent, RequestId};

    let (tx, mut rx) = broadcast::channel(16);
    let id = RequestId::from_raw(1);
    tx.send(ProxyEvent::TextDelta {
        id,
        text: "hello".to_string(),
    })?;

    let event = rx.recv().await?;
    match event {
        ProxyEvent::TextDelta { id: eid, text } => {
            assert_eq!(eid, id);
            assert_eq!(text, "hello");
        }
        other => anyhow::bail!("unexpected event: {other:?}"),
    }
    Ok(())
}

#[tokio::test]
async fn test_broadcast_multiple_receivers() -> anyhow::Result<()> {
    use crate::store::models::{ProxyEvent, RequestId};

    let (tx, mut rx1) = broadcast::channel(16);
    let mut rx2 = tx.subscribe();

    let id = RequestId::from_raw(2);
    tx.send(ProxyEvent::ResponseComplete {
        id,
        stop_reason: Some("end_turn".to_string()),
        stop_details: None,
        usage: None,
    })?;

    let e1 = rx1.recv().await?;
    let e2 = rx2.recv().await?;

    match (&e1, &e2) {
        (
            ProxyEvent::ResponseComplete {
                id: id1,
                stop_reason: sr1,
                ..
            },
            ProxyEvent::ResponseComplete {
                id: id2,
                stop_reason: sr2,
                ..
            },
        ) => {
            assert_eq!(*id1, id);
            assert_eq!(*id2, id);
            assert_eq!(sr1.as_deref(), Some("end_turn"));
            assert_eq!(sr2.as_deref(), Some("end_turn"));
        }
        _ => anyhow::bail!("unexpected events: {e1:?}, {e2:?}"),
    }
    Ok(())
}

#[tokio::test]
async fn test_lagged_receiver_continues() -> anyhow::Result<()> {
    use crate::store::models::{ProxyEvent, RequestId};
    use tokio::sync::broadcast::error::RecvError;

    let (tx, mut rx) = broadcast::channel::<ProxyEvent>(4);

    for i in 0..8 {
        let _ = tx.send(ProxyEvent::TextDelta {
            id: RequestId::from_raw(i),
            text: format!("msg-{i}"),
        });
    }

    let result = rx.recv().await;
    match result {
        Err(RecvError::Lagged(n)) => {
            assert!(n > 0, "should report skipped messages");
        }
        other => anyhow::bail!("expected Lagged, got: {other:?}"),
    }

    let event = rx.recv().await?;
    match event {
        ProxyEvent::TextDelta { text, .. } => {
            assert!(
                text.starts_with("msg-"),
                "should get a valid event after lag"
            );
        }
        other => anyhow::bail!("unexpected event: {other:?}"),
    }
    Ok(())
}

#[tokio::test]
async fn channel_link_attempted_starts_empty() -> anyhow::Result<()> {
    let config = parse(&["mike", "--persist-enabled", "0"])?;
    let state = AppState::new(config).await?;

    assert!(state.channel_link_attempted.is_empty());
    Ok(())
}
