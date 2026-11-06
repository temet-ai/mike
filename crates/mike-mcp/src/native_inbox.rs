use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;

use crate::client::{ClaimRoutes, ClaimedChannelMessage, InspectClient};

const SOCKET_ENV: &str = "CLAUDE_CODE_MESSAGING_SOCKET";
const TOKEN_ENV: &str = "CLAUDE_CODE_MESSAGING_TOKEN";
const ENVELOPE_TAG: &str = "cross-session-message";
const UNNAMED_SENDER: &str = "mike";
const SESSION_PREFIX_LEN: usize = 8;
const DELIVERY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeInbox {
    socket: PathBuf,
    token: Option<String>,
}

impl NativeInbox {
    pub(crate) fn from_env() -> Option<Self> {
        Self::from_values(
            std::env::var(SOCKET_ENV).ok(),
            std::env::var(TOKEN_ENV).ok(),
        )
    }

    fn from_values(socket: Option<String>, token: Option<String>) -> Option<Self> {
        let raw = socket?;
        let trimmed = raw.trim();
        let path = trimmed.strip_prefix("uds:").unwrap_or(trimmed);
        if !path.starts_with('/') {
            return None;
        }
        let token = token
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());
        Some(Self {
            socket: PathBuf::from(path),
            token,
        })
    }

    pub(crate) fn peer_address(&self) -> String {
        format!("uds:{}", self.socket.display())
    }

    fn lines(&self, message: &ClaimedChannelMessage) -> String {
        let mut out = String::new();
        if let Some(token) = &self.token {
            out.push_str(&json!({ "type": "auth", "token": token }).to_string());
            out.push('\n');
        }
        out.push_str(&socket_line(message).to_string());
        out.push('\n');
        out
    }

    pub(crate) async fn deliver(&self, message: &ClaimedChannelMessage) -> std::io::Result<()> {
        let payload = self.lines(message);
        let write = async {
            let mut stream = UnixStream::connect(&self.socket).await?;
            stream.write_all(payload.as_bytes()).await?;
            stream.shutdown().await
        };
        tokio::time::timeout(DELIVERY_TIMEOUT, write)
            .await
            .map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::TimedOut, "inbox socket write timed out")
            })?
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Delivery {
    pub(crate) inbox: Option<NativeInbox>,
    pub(crate) channel: bool,
}

impl Delivery {
    pub(crate) fn any(&self) -> bool {
        self.inbox.is_some() || self.channel
    }

    pub(crate) fn peer_address(&self) -> Option<String> {
        self.inbox.as_ref().map(NativeInbox::peer_address)
    }

    pub(crate) fn routes<'a>(&self, peer_address: Option<&'a str>) -> ClaimRoutes<'a> {
        ClaimRoutes {
            peer_address,
            channel: self.channel,
        }
    }
}

fn attribute_text(raw: &str) -> Option<String> {
    let cleaned: String = raw
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '"' | '<' | '>'))
        .collect();
    let trimmed = cleaned.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn is_session_uuid(raw: &str) -> bool {
    raw.len() == 36
        && raw.chars().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        })
}

fn peer_address_of(message: &ClaimedChannelMessage) -> Option<&str> {
    message.from_peer_address.as_deref().filter(|address| {
        address.starts_with("uds:/")
            && !address
                .chars()
                .any(|c| c.is_control() || c.is_whitespace() || matches!(c, '"' | '<' | '>'))
    })
}

fn sender_name(message: &ClaimedChannelMessage) -> String {
    let named = [&message.from_display_name, &message.from_role]
        .into_iter()
        .flatten()
        .find_map(|raw| attribute_text(raw));
    named
        .or_else(|| {
            message
                .from_session_uuid
                .as_deref()
                .and_then(|uuid| uuid.get(..SESSION_PREFIX_LEN))
                .map(str::to_string)
        })
        .unwrap_or_else(|| UNNAMED_SENDER.to_string())
}

pub(crate) fn envelope(message: &ClaimedChannelMessage) -> String {
    let mut attributes = Vec::new();
    if let Some(address) = peer_address_of(message) {
        attributes.push(format!("from=\"{address}\""));
    }
    if let Some(session) = message
        .from_session_uuid
        .as_deref()
        .filter(|s| is_session_uuid(s))
    {
        attributes.push(format!("from-session=\"{session}\""));
    }
    attributes.push(format!("from-name=\"{}\"", sender_name(message)));
    format!(
        "<{ENVELOPE_TAG} {}>\n{}\n</{ENVELOPE_TAG}>",
        attributes.join(" "),
        message.body
    )
}

pub(crate) fn socket_line(message: &ClaimedChannelMessage) -> Value {
    let mut line = json!({
        "type": "user",
        "message": { "role": "user", "content": envelope(message) },
        "priority": "next",
        "msg_id": message.message_uuid,
    });
    if let (Some(address), Some(fields)) = (peer_address_of(message), line.as_object_mut()) {
        fields.insert("from".into(), Value::String(address.to_string()));
    }
    line
}

pub(crate) async fn push_native_batch(
    inbox: &NativeInbox,
    client: &InspectClient,
    session_uuid: &str,
    messages: &[ClaimedChannelMessage],
) -> bool {
    for (index, message) in messages.iter().enumerate() {
        if let Err(error) = inbox.deliver(message).await {
            tracing::warn!(
                %session_uuid,
                %error,
                message_uuid = %message.message_uuid,
                "mike-mcp: posting mail to the session's inbox socket failed; releasing it and \
                 delivering by channel from now on"
            );
            crate::channel::release_unsent(client, session_uuid, &messages[index..]).await;
            return false;
        }
    }
    true
}

#[cfg(test)]
#[path = "native_inbox_tests.rs"]
mod tests;
