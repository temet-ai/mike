use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use uuid::Uuid;

const PROTOCOL_VERSION: u8 = 1;
const MAX_CONTROL_MESSAGE_BYTES: u64 = 8192;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct InstanceIdentity {
    pub(crate) instance_id: String,
    pub(crate) generation: Uuid,
    pub(crate) pid: u32,
    pub(crate) started_unix_secs: u64,
}

impl InstanceIdentity {
    pub(crate) fn verify_owner(&self, recorded: Option<&Self>, lease_held: bool) -> Result<()> {
        anyhow::ensure!(
            lease_held && recorded == Some(self),
            "private control identity does not match the current installation owner and generation; existing daemon was preserved"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct RuntimeStatus {
    pub(crate) identity: InstanceIdentity,
    pub(crate) ready: bool,
    pub(crate) stopping: bool,
    pub(crate) matches_requested: bool,
    pub(crate) persist_enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) bound_listeners: Option<Vec<std::net::SocketAddr>>,
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Action {
    Status,
    Stop,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    protocol: u8,
    instance_id: String,
    generation: Option<Uuid>,
    boot_fingerprint: Option<String>,
    action: Action,
}

impl Request {
    pub(crate) fn status(instance_id: String, boot_fingerprint: Option<String>) -> Self {
        Self {
            protocol: PROTOCOL_VERSION,
            instance_id,
            generation: None,
            boot_fingerprint,
            action: Action::Status,
        }
    }

    pub(crate) fn stop(identity: &InstanceIdentity) -> Self {
        Self {
            protocol: PROTOCOL_VERSION,
            instance_id: identity.instance_id.clone(),
            generation: Some(identity.generation),
            boot_fingerprint: None,
            action: Action::Stop,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Rejection {
    #[serde(rename = "protocol_mismatch")]
    Protocol,
    #[serde(rename = "instance_mismatch")]
    Instance,
    #[serde(rename = "generation_mismatch")]
    Generation,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Response {
    pub(crate) status: Option<RuntimeStatus>,
    pub(crate) rejection: Option<Rejection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) config_file: Option<PathBuf>,
}

impl Response {
    pub(crate) fn into_status(self) -> Result<RuntimeStatus> {
        self.status.with_context(|| {
            format!(
                "private daemon control rejected the request ({:?})",
                self.rejection
            )
        })
    }

    pub(crate) fn into_status_for(self, requesting: &Path) -> Result<RuntimeStatus> {
        if self.rejection == Some(Rejection::Instance) {
            anyhow::bail!(instance_mismatch(requesting, self.config_file.as_deref()));
        }
        self.into_status()
    }
}

pub(crate) fn instance_mismatch(requesting: &Path, daemon: Option<&Path>) -> String {
    let daemon = daemon.map_or_else(
        || "a config file this daemon does not report (it predates this check)".to_string(),
        |file| file.display().to_string(),
    );
    format!(
        "this command is using a different mike installation from the running daemon: the command's config file is {}, the daemon's is {daemon}. Run the command with the daemon's config folder (MIKE_CONFIG_HOME or --config)",
        requesting.display()
    )
}

pub(crate) struct Decision<'a> {
    pub(crate) identity: &'a InstanceIdentity,
    pub(crate) config_file: &'a Path,
    pub(crate) boot_fingerprint: &'a str,
    pub(crate) ready: bool,
    pub(crate) stopping: bool,
    pub(crate) persist_enabled: bool,
    pub(crate) bound_listeners: Option<&'a [std::net::SocketAddr]>,
}

impl Decision<'_> {
    pub(crate) fn reply(&self, request: &Request) -> (Response, bool) {
        if let Some(rejection) = self.reject(request) {
            return (
                Response {
                    status: None,
                    rejection: Some(rejection),
                    config_file: (rejection == Rejection::Instance)
                        .then(|| self.config_file.to_path_buf()),
                },
                false,
            );
        }
        let stopping = request.action == Action::Stop;
        let status = RuntimeStatus {
            identity: self.identity.clone(),
            ready: self.ready,
            stopping: self.stopping || stopping,
            matches_requested: request
                .boot_fingerprint
                .as_deref()
                .is_none_or(|wanted| wanted == self.boot_fingerprint),
            persist_enabled: self.persist_enabled,
            bound_listeners: self.bound_listeners.map(<[_]>::to_vec),
        };
        (
            Response {
                status: Some(status),
                rejection: None,
                config_file: None,
            },
            stopping,
        )
    }

    fn reject(&self, request: &Request) -> Option<Rejection> {
        if request.protocol != PROTOCOL_VERSION {
            return Some(Rejection::Protocol);
        }
        if request.instance_id != self.identity.instance_id {
            return Some(Rejection::Instance);
        }
        if request.action == Action::Stop && request.generation != Some(self.identity.generation) {
            return Some(Rejection::Generation);
        }
        None
    }
}

pub(crate) async fn read<T: serde::de::DeserializeOwned>(
    stream: &mut (impl AsyncRead + Unpin),
) -> Result<T> {
    let mut bytes = Vec::new();
    stream
        .take(MAX_CONTROL_MESSAGE_BYTES + 1)
        .read_to_end(&mut bytes)
        .await
        .context("cannot read private daemon control message")?;
    anyhow::ensure!(
        bytes.len() as u64 <= MAX_CONTROL_MESSAGE_BYTES,
        "private daemon control message exceeds protocol limit"
    );
    serde_json::from_slice(&bytes)
        .map_err(|_| anyhow::anyhow!("invalid private daemon control message; contents omitted"))
}

pub(crate) async fn write<T: Serialize>(
    stream: &mut (impl AsyncWrite + Unpin),
    value: &T,
) -> Result<()> {
    let bytes =
        serde_json::to_vec(value).context("cannot encode private daemon control message")?;
    anyhow::ensure!(
        bytes.len() as u64 <= MAX_CONTROL_MESSAGE_BYTES,
        "private daemon control message exceeds protocol limit"
    );
    stream
        .write_all(&bytes)
        .await
        .context("cannot write private daemon control message")?;
    stream
        .shutdown()
        .await
        .context("cannot finish private daemon control message")
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
