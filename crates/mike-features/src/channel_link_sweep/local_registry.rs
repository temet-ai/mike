use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use uuid::Uuid;

use crate::proxy::remote_control::route::is_channel_id;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::launch::LinkOutcome;

const REGISTRY_FOLDER: &str = "sessions";
const BRIDGE_ID_PREFIX: &str = "session_";
const CHANNEL_ID_PREFIX: &str = "cse_";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegistryEntry {
    session_id: Uuid,
    bridge_session_id: Option<String>,
    #[serde(default)]
    cwd: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct RunningChannel {
    pub cse_id: String,
    pub session_uuid: Uuid,
    pub cwd: String,
}

fn parse_entry(bytes: &[u8]) -> Option<RunningChannel> {
    let entry: RegistryEntry = serde_json::from_slice(bytes).ok()?;
    let bridge_id = entry.bridge_session_id?;
    let cse_id = format!(
        "{CHANNEL_ID_PREFIX}{}",
        bridge_id.strip_prefix(BRIDGE_ID_PREFIX)?
    );
    is_channel_id(&cse_id).then_some(RunningChannel {
        cse_id,
        session_uuid: entry.session_id,
        cwd: entry.cwd,
    })
}

fn read_running_channels(folder: &Path) -> Vec<RunningChannel> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .filter_map(|path| std::fs::read(path).ok())
        .filter_map(|bytes| parse_entry(&bytes))
        .collect()
}

async fn running_channels(folder: PathBuf) -> Vec<RunningChannel> {
    tokio::task::spawn_blocking(move || read_running_channels(&folder))
        .await
        .unwrap_or_default()
}

/// Channels this process has seen linked to their session. A session file
/// names the same pair on every tick, so a linked pair is not asked again
/// while its file stays.
#[derive(Default)]
struct LinkedChannels(std::sync::Mutex<HashSet<(String, Uuid)>>);

fn linked_pairs(persist: &PersistHandle) -> std::sync::Arc<LinkedChannels> {
    persist.memo::<LinkedChannels>()
}

fn still_running(known: &LinkedChannels, running: &[RunningChannel]) -> Vec<(String, Uuid)> {
    let current: HashSet<(String, Uuid)> = running
        .iter()
        .map(|channel| (channel.cse_id.clone(), channel.session_uuid))
        .collect();
    let mut linked = known
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    linked.retain(|pair| current.contains(pair));
    current
        .into_iter()
        .filter(|pair| !linked.contains(pair))
        .collect()
}

fn remember(known: &LinkedChannels, pair: (String, Uuid)) {
    known
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(pair);
}

pub async fn link_running_sessions(persist: &PersistHandle) -> usize {
    let Some(home) = crate::knowledge_scan::claude_home() else {
        return 0;
    };
    let running = running_channels(home.join(REGISTRY_FOLDER)).await;
    let known = linked_pairs(persist);
    let unlinked = still_running(&known, &running);
    let mut linked = 0;
    for channel in running
        .iter()
        .filter(|channel| unlinked.contains(&(channel.cse_id.clone(), channel.session_uuid)))
    {
        match persist
            .launch()
            .link_channel_to_session(&channel.cse_id, channel.session_uuid, &channel.cwd)
            .await
        {
            Ok(outcome @ (LinkOutcome::Linked | LinkOutcome::AlreadyLinked)) => {
                linked += usize::from(outcome == LinkOutcome::Linked);
                remember(&known, (channel.cse_id.clone(), channel.session_uuid));
            }
            Ok(LinkOutcome::NoChannel) => {}
            Err(e) => {
                tracing::warn!(
                    cse_id = %channel.cse_id,
                    session_id = %channel.session_uuid,
                    error = %e,
                    "channel_link_sweep: running-session link failed"
                );
            }
        }
    }
    linked
}

#[cfg(test)]
mod tests {
    use super::*;

    const SESSION: &str = "48200218-5859-41ea-bba3-fe525738d51b";

    fn entry(bridge: &str) -> String {
        format!(r#"{{"pid":1,"sessionId":"{SESSION}","cwd":"/work","bridgeSessionId":"{bridge}"}}"#)
    }

    #[test]
    fn parse_entry_with_a_bridge_id_gives_the_channel_and_session() -> anyhow::Result<()> {
        let parsed = parse_entry(entry("session_01AbC").as_bytes());
        assert_eq!(
            parsed,
            Some(RunningChannel {
                cse_id: "cse_01AbC".to_owned(),
                session_uuid: Uuid::parse_str(SESSION)?,
                cwd: "/work".to_owned(),
            })
        );
        Ok(())
    }

    #[test]
    fn parse_entry_without_a_bridge_id_gives_nothing() {
        let body = format!(r#"{{"pid":1,"sessionId":"{SESSION}","cwd":"/work"}}"#);
        assert_eq!(parse_entry(body.as_bytes()), None);
    }

    #[test]
    fn parse_entry_with_a_foreign_bridge_id_gives_nothing() {
        assert_eq!(parse_entry(entry("cse_01AbC").as_bytes()), None);
        assert_eq!(parse_entry(entry("session_01/../x").as_bytes()), None);
    }

    fn running(cse_id: &str, session: Uuid) -> RunningChannel {
        RunningChannel {
            cse_id: cse_id.to_owned(),
            session_uuid: session,
            cwd: "/work".to_owned(),
        }
    }

    #[test]
    fn still_running_skips_linked_pairs_and_forgets_gone_ones() -> anyhow::Result<()> {
        let session = Uuid::parse_str(SESSION)?;
        let known = LinkedChannels::default();
        remember(&known, ("cse_linked".to_owned(), session));
        remember(&known, ("cse_gone".to_owned(), session));

        let ask = still_running(
            &known,
            &[running("cse_linked", session), running("cse_new", session)],
        );

        assert_eq!(ask, vec![("cse_new".to_owned(), session)]);
        let kept = known.0.lock().map_err(|_| anyhow::anyhow!("poisoned"))?;
        assert!(kept.contains(&("cse_linked".to_owned(), session)));
        assert!(!kept.contains(&("cse_gone".to_owned(), session)));
        Ok(())
    }

    #[test]
    fn parse_entry_with_broken_json_gives_nothing() {
        assert_eq!(parse_entry(b"{not json"), None);
    }

    #[test]
    fn read_running_channels_reads_only_json_entries_with_a_bridge() -> anyhow::Result<()> {
        let folder = tempfile::tempdir()?;
        std::fs::write(folder.path().join("1.json"), entry("session_01AbC"))?;
        std::fs::write(folder.path().join("2.json"), "{}")?;
        std::fs::write(folder.path().join("3.tmp"), entry("session_02DeF"))?;

        let channels = read_running_channels(folder.path());

        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].cse_id, "cse_01AbC");
        Ok(())
    }

    #[test]
    fn read_running_channels_with_no_folder_gives_nothing() -> anyhow::Result<()> {
        let folder = tempfile::tempdir()?;
        assert!(read_running_channels(&folder.path().join("missing")).is_empty());
        Ok(())
    }
}
