use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tokio::time::{MissedTickBehavior, interval};
use tokio_util::sync::CancellationToken;

use super::RequestStore;

#[derive(Debug, Clone)]
pub struct RetentionPolicy {
    pub max_entries: usize,
    pub max_age: Duration,
    pub prune_interval: Duration,
    pub max_store_bytes: u64,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            max_entries: 1000,
            max_age: Duration::from_secs(3600),
            prune_interval: Duration::from_secs(60),
            max_store_bytes: 3 * 1024 * 1024 * 1024,
        }
    }
}

impl RetentionPolicy {
    pub fn parse_duration(s: &str) -> Duration {
        let trimmed = s.trim();
        match humantime::parse_duration(trimmed) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(
                    value = %trimmed,
                    error = %e,
                    "failed to parse duration — defaulting to 1h"
                );
                Duration::from_secs(3600)
            }
        }
    }
}

pub fn spawn_pruner(
    store: Arc<RwLock<RequestStore>>,
    retention: RetentionPolicy,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = interval(retention.prune_interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        ticker.tick().await;

        loop {
            let should_prune = tokio::select! {
                _ = ticker.tick() => true,
                _ = cancel.cancelled() => {
                    tracing::debug!("pruner cancelled");
                    break;
                }
            };
            if should_prune {
                let cutoff = chrono::Utc::now()
                    - chrono::Duration::from_std(retention.max_age)
                        .unwrap_or_else(|_| chrono::Duration::seconds(3600));
                let mut store = store.write().await;

                let pruned = store.prune_before(cutoff);
                if pruned > 0 {
                    tracing::info!(
                        pruned,
                        max_age_secs = retention.max_age.as_secs(),
                        "pruned expired requests"
                    );
                    metrics::counter!(
                        crate::metrics::COUNTER_STORE_EVICTIONS_TOTAL,
                        "reason" => "age",
                    )
                    .increment(pruned as u64);
                }

                let outcome = store.evict_to_byte_budget();
                if outcome.entries_evicted > 0 {
                    tracing::info!(
                        entries_evicted = outcome.entries_evicted,
                        bytes_freed = outcome.bytes_freed,
                        bytes_freed_human = %bytesize::ByteSize::b(outcome.bytes_freed),
                        bytes_remaining_after = outcome.bytes_remaining_after,
                        bytes_remaining_after_human = %bytesize::ByteSize::b(outcome.bytes_remaining_after),
                        budget = outcome.budget,
                        budget_human = %bytesize::ByteSize::b(outcome.budget),
                        "pruner enforced byte budget"
                    );
                }
                if outcome.over_budget() {
                    tracing::warn!(
                        bytes_remaining_after = outcome.bytes_remaining_after,
                        bytes_remaining_after_human = %bytesize::ByteSize::b(outcome.bytes_remaining_after),
                        budget = outcome.budget,
                        budget_human = %bytesize::ByteSize::b(outcome.budget),
                        entries_evicted = outcome.entries_evicted,
                        "in-memory store still over byte budget after pruner sweep — every remaining entry is non-terminal or pinned; pin a long-running session to keep it in memory or wait for streams to terminate"
                    );
                }
            }
        }
    })
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::store::models::{InterceptedRequest, RequestId, RequestStatus};
    use crate::types::RequestParams;

    #[test]
    fn parse_duration_basic_units() {
        assert_eq!(
            RetentionPolicy::parse_duration("1h"),
            Duration::from_secs(3600)
        );
        assert_eq!(
            RetentionPolicy::parse_duration("30m"),
            Duration::from_secs(1800)
        );
        assert_eq!(
            RetentionPolicy::parse_duration("10s"),
            Duration::from_secs(10)
        );
    }

    #[test]
    fn parse_duration_humantime_superset() {
        assert_eq!(
            RetentionPolicy::parse_duration("1h30m"),
            Duration::from_secs(3600 + 30 * 60)
        );
        assert_eq!(
            RetentionPolicy::parse_duration("500ms"),
            Duration::from_millis(500)
        );
    }

    #[test]
    fn parse_duration_invalid_defaults_to_1h() {
        assert_eq!(
            RetentionPolicy::parse_duration("invalid"),
            Duration::from_secs(3600)
        );
        assert_eq!(
            RetentionPolicy::parse_duration(""),
            Duration::from_secs(3600)
        );
        assert_eq!(
            RetentionPolicy::parse_duration("h"),
            Duration::from_secs(3600)
        );
    }

    #[test]
    fn parse_duration_trims_whitespace() {
        assert_eq!(
            RetentionPolicy::parse_duration("  2h  "),
            Duration::from_secs(7200)
        );
    }

    fn make_old_complete_request(session_id: &str) -> InterceptedRequest {
        InterceptedRequest {
            id: RequestId::next(),
            upstream_request_id: None,
            upstream_message_id: None,
            session_id: session_id.to_string(),
            method: "POST".to_string(),
            path: "/v1/messages".to_string(),
            request_headers: vec![],
            request_body: None,
            model: Some("claude-sonnet-4-20250514".to_string()),
            is_streaming: false,
            message_count: 1,
            is_tool_loop: false,
            last_user_message: "hello".to_string(),
            tool_names: vec![],
            params: RequestParams {
                max_tokens: Some(1024),
                temperature: None,
                thinking_enabled: false,
                thinking_budget: None,
                thinking_mode: None,
                effort: None,
            },
            project: None,
            git_repo: None,
            human_prompt: String::new(),
            turn_type: "user_prompt".to_string(),
            turn_origin: "primary".to_string(),
            subagent_persona: None,
            status: RequestStatus::Complete,
            response_status: Some(200),
            response_headers: None,
            response_body: None,
            tool_calls: vec![],
            token_usage: None,
            stop_reason: None,
            stop_details: None,
            message_fields: None,
            tool_results: Vec::new(),
            tool_results_in_body: Vec::new(),
            failed_stream_blocks: Vec::new(),
            expected_stream_block_count: 0,
            created_at: chrono::Utc::now() - chrono::Duration::hours(2),
            completed_at: None,
            diagnostics: Vec::new(),
            filter_applied: false,
            filtered_request_body: None,
            sent_body: None,
            transformation_manifest: None,
            system_prompt: crate::store::models::SystemPromptObservation::default(),
            intercepted: false,
            correction_request_id: None,
            agent_instance_id: None,
            claude_code_agent_id: None,
        }
    }

    #[tokio::test]
    async fn pruner_removes_old_entries() -> anyhow::Result<()> {
        let retention = RetentionPolicy {
            max_entries: 100,
            max_age: Duration::from_secs(1),
            prune_interval: Duration::from_millis(100),
            max_store_bytes: 0,
        };

        let store = Arc::new(RwLock::new(RequestStore::new(RetentionPolicy {
            max_entries: 100,
            ..RetentionPolicy::default()
        })));

        {
            let mut s = store.write().await;
            let req = make_old_complete_request("sess-1");
            s.insert(req);
            assert_eq!(s.len(), 1);
        }

        let cancel = CancellationToken::new();
        let handle = spawn_pruner(store.clone(), retention, cancel.clone());

        tokio::time::sleep(Duration::from_millis(300)).await;

        cancel.cancel();
        handle.await?;

        let s = store.read().await;
        assert_eq!(s.len(), 0, "old complete entry should have been pruned");

        Ok(())
    }

    #[tokio::test]
    async fn pruner_stops_on_cancel() -> anyhow::Result<()> {
        let retention = RetentionPolicy {
            max_entries: 100,
            max_age: Duration::from_secs(3600),
            prune_interval: Duration::from_millis(50),
            max_store_bytes: 0,
        };

        let store = Arc::new(RwLock::new(RequestStore::new(RetentionPolicy::default())));
        let cancel = CancellationToken::new();
        let handle = spawn_pruner(store, retention, cancel.clone());

        cancel.cancel();

        let result = tokio::time::timeout(Duration::from_secs(2), handle).await;
        assert!(result.is_ok(), "pruner should exit promptly after cancel");

        Ok(())
    }
}
