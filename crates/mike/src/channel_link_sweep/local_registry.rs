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

pub async fn link_running_sessions(persist: &PersistHandle) -> usize {
    let Some(home) = crate::knowledge_scan::claude_home() else {
        return 0;
    };
    let mut linked = 0;
    for channel in running_channels(home.join(REGISTRY_FOLDER)).await {
        match persist
            .link_channel_to_session(&channel.cse_id, channel.session_uuid, &channel.cwd)
            .await
        {
            Ok(LinkOutcome::Linked) => linked += 1,
            Ok(LinkOutcome::AlreadyLinked | LinkOutcome::NoChannel) => {}
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
