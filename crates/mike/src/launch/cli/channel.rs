use std::future::Future;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use uuid::Uuid;

use super::ledger::Ledger;
use crate::config::LaunchConfig;
use crate::launch::{ChannelCheck, channel_link};
use crate::store::persistence::launch::LinkOutcome;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChannelBudget {
    pub(crate) enabled: bool,
    pub(crate) timeout: Duration,
    pub(crate) interval: Duration,
}

impl ChannelBudget {
    pub(crate) fn from_config(config: &LaunchConfig) -> Self {
        Self {
            enabled: config.channel_check_enabled,
            timeout: Duration::from_secs(config.channel_check_timeout_secs),
            interval: Duration::from_secs(config.channel_check_interval_secs.max(1)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChannelSource {
    pub(crate) transcript: Option<PathBuf>,
    pub(crate) cwd: String,
}

pub(crate) trait ChannelStore {
    fn available(&self) -> bool;
    async fn present(&self, session: Uuid) -> Result<bool, String>;
    async fn link(&self, cse_id: &str, session: Uuid, cwd: &str) -> Result<LinkOutcome, String>;
}

impl ChannelStore for Ledger {
    fn available(&self) -> bool {
        Ledger::available(self)
    }

    async fn present(&self, session: Uuid) -> Result<bool, String> {
        self.channel_present(session).await
    }

    async fn link(&self, cse_id: &str, session: Uuid, cwd: &str) -> Result<LinkOutcome, String> {
        self.link_channel(cse_id, session, cwd).await
    }
}

pub(crate) fn as_str(check: ChannelCheck) -> &'static str {
    match check {
        ChannelCheck::Found => "found",
        ChannelCheck::Timeout => "timeout",
        ChannelCheck::DbUnreachable => "db_unreachable",
        ChannelCheck::NoSessionId => "no_session_id",
        ChannelCheck::Disabled => "disabled",
    }
}

pub(crate) async fn check<S: ChannelStore>(
    store: &S,
    session: Option<Uuid>,
    source: &ChannelSource,
    budget: &ChannelBudget,
) -> ChannelCheck {
    if !budget.enabled {
        return ChannelCheck::Disabled;
    }
    let Some(id) = session else {
        return ChannelCheck::NoSessionId;
    };
    if !store.available() {
        return ChannelCheck::DbUnreachable;
    }
    let warned = AtomicBool::new(false);
    poll(budget, || probe_once(store, id, source, &warned)).await
}

async fn probe_once<S: ChannelStore>(
    store: &S,
    session: Uuid,
    source: &ChannelSource,
    warned: &AtomicBool,
) -> Result<bool, String> {
    if store.present(session).await? {
        return Ok(true);
    }
    let Some(path) = &source.transcript else {
        return Ok(false);
    };
    let cse_id = match channel_link::read_bridge_cse_id(path) {
        Ok(Some(id)) => id,
        Ok(None) => return Ok(false),
        Err(e) => {
            if !warned.swap(true, Ordering::Relaxed) {
                eprintln!("mike spawn: channel check cannot read the transcript: {e}");
            }
            return Ok(false);
        }
    };
    match store.link(&cse_id, session, &source.cwd).await? {
        LinkOutcome::Linked => {
            eprintln!("mike spawn: channel: linked {cse_id} to {session} from the transcript");
            Ok(true)
        }
        LinkOutcome::AlreadyLinked => Ok(true),
        LinkOutcome::NoChannel => Ok(false),
    }
}

pub(crate) async fn poll<F, Fut>(budget: &ChannelBudget, mut probe: F) -> ChannelCheck
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<bool, String>>,
{
    let started = Instant::now();
    let mut answered = false;
    let mut last_error = None;
    loop {
        match probe().await {
            Ok(true) => return ChannelCheck::Found,
            Ok(false) => answered = true,
            Err(e) => last_error = Some(e),
        }
        if started.elapsed() >= budget.timeout {
            break;
        }
        tokio::time::sleep(budget.interval).await;
    }
    if answered {
        ChannelCheck::Timeout
    } else {
        if let Some(e) = last_error {
            eprintln!("mike spawn: channel check could not reach the database: {e}");
        }
        ChannelCheck::DbUnreachable
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    const BRIDGE: &str = r#"{"type":"system","subtype":"bridge_status","url":"https://claude.ai/code/session_01ABC"}"#;

    fn budget(timeout_ms: u64) -> ChannelBudget {
        ChannelBudget {
            enabled: true,
            timeout: Duration::from_millis(timeout_ms),
            interval: Duration::from_millis(5),
        }
    }

    fn no_source() -> ChannelSource {
        ChannelSource {
            transcript: None,
            cwd: "/tmp/project".into(),
        }
    }

    #[derive(Default)]
    struct ScriptedStore {
        present: Mutex<VecDeque<bool>>,
        link_answer: Mutex<Option<LinkOutcome>>,
        links: Mutex<Vec<(String, Uuid, String)>>,
    }

    impl ChannelStore for ScriptedStore {
        fn available(&self) -> bool {
            true
        }

        async fn present(&self, _session: Uuid) -> Result<bool, String> {
            let mut queue = self.present.lock().map_err(|e| e.to_string())?;
            Ok(match queue.len() {
                0 => false,
                1 => queue[0],
                _ => queue.pop_front().unwrap_or(false),
            })
        }

        async fn link(
            &self,
            cse_id: &str,
            session: Uuid,
            cwd: &str,
        ) -> Result<LinkOutcome, String> {
            self.links.lock().map_err(|e| e.to_string())?.push((
                cse_id.to_string(),
                session,
                cwd.to_string(),
            ));
            Ok(self
                .link_answer
                .lock()
                .map_err(|e| e.to_string())?
                .unwrap_or(LinkOutcome::NoChannel))
        }
    }

    fn transcript(dir: &std::path::Path, text: &str) -> ChannelSource {
        let path = dir.join("s.jsonl");
        std::fs::write(&path, text).expect("transcript fixture");
        ChannelSource {
            transcript: Some(path),
            cwd: "/tmp/project".into(),
        }
    }

    #[tokio::test]
    async fn check_channel_present_at_once_is_found_without_reading_the_transcript()
    -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let store = ScriptedStore::default();
        store
            .present
            .lock()
            .map_err(|e| anyhow::anyhow!("poisoned lock: {e}"))?
            .push_back(true);
        let source = transcript(dir.path(), BRIDGE);
        let result = check(&store, Some(Uuid::nil()), &source, &budget(500)).await;
        assert_eq!(result, ChannelCheck::Found);
        assert!(
            store
                .links
                .lock()
                .map_err(|e| anyhow::anyhow!("poisoned lock: {e}"))?
                .is_empty()
        );
        Ok(())
    }

    #[tokio::test]
    async fn check_transcript_names_the_channel_and_the_link_is_written_is_found()
    -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let store = ScriptedStore::default();
        *store
            .link_answer
            .lock()
            .map_err(|e| anyhow::anyhow!("poisoned lock: {e}"))? = Some(LinkOutcome::Linked);
        let session = Uuid::new_v4();
        let source = transcript(dir.path(), &format!("{{\"type\":\"user\"}}\n{BRIDGE}\n"));
        let result = check(&store, Some(session), &source, &budget(500)).await;
        assert_eq!(result, ChannelCheck::Found);
        let links = store
            .links
            .lock()
            .map_err(|e| anyhow::anyhow!("poisoned lock: {e}"))?
            .clone();
        assert_eq!(
            links,
            vec![("cse_01ABC".to_string(), session, "/tmp/project".to_string())]
        );
        Ok(())
    }

    #[tokio::test]
    async fn check_channel_already_linked_is_found() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let store = ScriptedStore::default();
        *store
            .link_answer
            .lock()
            .map_err(|e| anyhow::anyhow!("poisoned lock: {e}"))? = Some(LinkOutcome::AlreadyLinked);
        let source = transcript(dir.path(), BRIDGE);
        let result = check(&store, Some(Uuid::nil()), &source, &budget(500)).await;
        assert_eq!(result, ChannelCheck::Found);
        Ok(())
    }

    #[tokio::test]
    async fn check_no_channel_row_until_the_budget_ends_is_timeout_with_repeated_link_attempts()
    -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let store = ScriptedStore::default();
        let source = transcript(dir.path(), BRIDGE);
        let result = check(&store, Some(Uuid::nil()), &source, &budget(40)).await;
        assert_eq!(result, ChannelCheck::Timeout);
        assert!(
            store
                .links
                .lock()
                .map_err(|e| anyhow::anyhow!("poisoned lock: {e}"))?
                .len()
                >= 2
        );
        Ok(())
    }

    #[tokio::test]
    async fn check_transcript_absent_or_without_bridge_line_is_timeout_without_a_link()
    -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let store = ScriptedStore::default();
        let missing = ChannelSource {
            transcript: Some(dir.path().join("absent.jsonl")),
            cwd: "/tmp/project".into(),
        };
        assert_eq!(
            check(&store, Some(Uuid::nil()), &missing, &budget(20)).await,
            ChannelCheck::Timeout
        );
        let no_bridge = transcript(dir.path(), "{\"type\":\"user\"}\n");
        assert_eq!(
            check(&store, Some(Uuid::nil()), &no_bridge, &budget(20)).await,
            ChannelCheck::Timeout
        );
        assert_eq!(
            check(&store, Some(Uuid::nil()), &no_source(), &budget(20)).await,
            ChannelCheck::Timeout
        );
        assert!(
            store
                .links
                .lock()
                .map_err(|e| anyhow::anyhow!("poisoned lock: {e}"))?
                .is_empty()
        );
        Ok(())
    }

    #[tokio::test]
    async fn poll_found_on_a_later_attempt_is_found() {
        let calls = AtomicUsize::new(0);
        let result = poll(&budget(2_000), || {
            let n = calls.fetch_add(1, Ordering::SeqCst);
            async move { Ok(n >= 2) }
        })
        .await;
        assert_eq!(result, ChannelCheck::Found);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn poll_database_answered_no_until_budget_is_timeout() {
        let result = poll(&budget(30), || async { Ok(false) }).await;
        assert_eq!(result, ChannelCheck::Timeout);
    }

    #[tokio::test]
    async fn poll_database_never_answered_is_db_unreachable() {
        let result = poll(&budget(30), || async { Err("refused".to_string()) }).await;
        assert_eq!(result, ChannelCheck::DbUnreachable);
    }

    #[tokio::test]
    async fn poll_one_answer_among_errors_is_timeout_not_unreachable() {
        let calls = AtomicUsize::new(0);
        let result = poll(&budget(40), || {
            let n = calls.fetch_add(1, Ordering::SeqCst);
            async move {
                if n == 0 {
                    Ok(false)
                } else {
                    Err("refused".to_string())
                }
            }
        })
        .await;
        assert_eq!(result, ChannelCheck::Timeout);
    }

    #[tokio::test]
    async fn check_disabled_no_session_and_no_ledger_short_circuit() {
        let ledger = Ledger::none();
        let disabled = ChannelBudget {
            enabled: false,
            ..budget(10)
        };
        assert_eq!(
            check(&ledger, Some(Uuid::nil()), &no_source(), &disabled).await,
            ChannelCheck::Disabled
        );
        assert_eq!(
            check(&ledger, None, &no_source(), &budget(10)).await,
            ChannelCheck::NoSessionId
        );
        assert_eq!(
            check(&ledger, Some(Uuid::nil()), &no_source(), &budget(10)).await,
            ChannelCheck::DbUnreachable
        );
    }

    #[test]
    fn budget_from_config_floors_the_interval_at_one_second() {
        let config = LaunchConfig {
            channel_check_interval_secs: 0,
            channel_check_timeout_secs: 7,
            ..LaunchConfig::default()
        };
        let b = ChannelBudget::from_config(&config);
        assert_eq!(b.interval, Duration::from_secs(1));
        assert_eq!(b.timeout, Duration::from_secs(7));
        assert!(b.enabled);
    }

    #[test]
    fn as_str_matches_the_serde_spelling() -> anyhow::Result<()> {
        for c in [
            ChannelCheck::Found,
            ChannelCheck::Timeout,
            ChannelCheck::DbUnreachable,
            ChannelCheck::NoSessionId,
            ChannelCheck::Disabled,
        ] {
            assert_eq!(serde_json::to_value(c)?, serde_json::Value::from(as_str(c)));
        }
        Ok(())
    }
}
