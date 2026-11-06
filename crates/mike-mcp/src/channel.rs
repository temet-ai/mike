use std::collections::BTreeMap;
use std::time::Duration;

use rmcp::model::{CustomNotification, ServerNotification};
use rmcp::{Peer, RoleServer, ServiceError};
use serde_json::json;

use crate::client::ClaimedChannelMessage;
use crate::client::InspectClient;
use crate::native_inbox::{Delivery, push_native_batch};

pub(crate) const CHANNEL_NOTIFICATION_METHOD: &str = "notifications/claude/channel";
pub(crate) const CHANNEL_EXPERIMENTAL_KEY: &str = "claude/channel";

const POLL_INTERVAL: Duration = Duration::from_secs(2);
const MAX_BACKOFF: Duration = Duration::from_secs(10);
const NOTIFICATION_KIND: &str = "mailbox_message";
const CHANNEL_SERVER_ENTRY: &str = "server:mike";
const CHANNEL_FLAGS: [&str; 2] = ["--dangerously-load-development-channels", "--channels"];
const CHANNEL_OVERRIDE_ENV: &str = "MIKE_MCP_CHANNEL";

fn channel_flag_values(arg: &str) -> Option<Option<&str>> {
    CHANNEL_FLAGS.iter().find_map(|flag| {
        if arg == *flag {
            return Some(None);
        }
        arg.strip_prefix(flag)
            .and_then(|rest| rest.strip_prefix('='))
            .map(Some)
    })
}

pub(crate) fn argv_opts_into_channel(args: &[String]) -> bool {
    let mut collecting = false;
    for arg in args {
        if let Some(inline) = channel_flag_values(arg) {
            collecting = inline.is_none();
            if inline.is_some_and(|v| v.split(',').any(|e| e == CHANNEL_SERVER_ENTRY)) {
                return true;
            }
            continue;
        }
        if arg.starts_with("--") {
            collecting = false;
            continue;
        }
        if collecting && arg.split(',').any(|e| e == CHANNEL_SERVER_ENTRY) {
            return true;
        }
    }
    false
}

pub(crate) fn override_decision(value: Option<&str>) -> Option<bool> {
    match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
        Some("on" | "1" | "true") => Some(true),
        Some("off" | "0" | "false") => Some(false),
        _ => None,
    }
}

fn parent_argv() -> Vec<String> {
    let ppid = std::os::unix::process::parent_id();
    if let Ok(raw) = std::fs::read(format!("/proc/{ppid}/cmdline")) {
        return raw
            .split(|b| *b == 0)
            .filter(|part| !part.is_empty())
            .map(|part| String::from_utf8_lossy(part).into_owned())
            .collect();
    }
    std::process::Command::new("ps")
        .args(["-o", "args=", "-p", &ppid.to_string()])
        .output()
        .ok()
        .map(|out| {
            String::from_utf8_lossy(&out.stdout)
                .split_whitespace()
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn client_opted_into_channel() -> bool {
    let env_value = std::env::var(CHANNEL_OVERRIDE_ENV).ok();
    override_decision(env_value.as_deref())
        .unwrap_or_else(|| argv_opts_into_channel(&parent_argv()))
}

pub(crate) struct PollState {
    backoff: Duration,
    in_outage: bool,
}

impl Default for PollState {
    fn default() -> Self {
        Self {
            backoff: POLL_INTERVAL,
            in_outage: false,
        }
    }
}

impl PollState {
    pub(crate) fn on_success(&mut self) -> Duration {
        self.backoff = POLL_INTERVAL;
        let recovered = self.in_outage;
        self.in_outage = false;
        if recovered {
            tracing::info!("mike-mcp: channel poll recovered");
        }
        POLL_INTERVAL
    }

    pub(crate) fn on_failure(&mut self) -> Duration {
        let wait = self.backoff;
        if !self.in_outage {
            self.in_outage = true;
        }
        self.backoff = (self.backoff * 2).min(MAX_BACKOFF);
        wait
    }

    pub(crate) fn should_log_failure(&self) -> bool {
        !self.in_outage
    }
}

fn is_valid_meta_key(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn insert_meta(meta: &mut BTreeMap<String, String>, key: &'static str, value: Option<String>) {
    if !is_valid_meta_key(key) {
        return;
    }
    if let Some(value) = value {
        meta.insert(key.to_string(), value);
    }
}

pub(crate) fn build_notification(
    message: &ClaimedChannelMessage,
) -> (String, BTreeMap<String, String>) {
    let mut meta = BTreeMap::new();
    insert_meta(&mut meta, "message_id", Some(message.message_uuid.clone()));
    insert_meta(&mut meta, "from_session", message.from_session_uuid.clone());
    insert_meta(&mut meta, "from_name", message.from_display_name.clone());
    insert_meta(&mut meta, "from_role", message.from_role.clone());
    insert_meta(&mut meta, "kind", Some(NOTIFICATION_KIND.to_string()));
    (message.envelope.clone(), meta)
}

fn notification_for(message: &ClaimedChannelMessage) -> ServerNotification {
    let (content, meta) = build_notification(message);
    ServerNotification::CustomNotification(CustomNotification::new(
        CHANNEL_NOTIFICATION_METHOD,
        Some(json!({ "content": content, "meta": meta })),
    ))
}

enum BatchOutcome {
    Delivered,
    TransportClosed,
}

async fn push_batch(
    peer: &Peer<RoleServer>,
    client: &InspectClient,
    session_uuid: &str,
    messages: &[ClaimedChannelMessage],
) -> BatchOutcome {
    for (index, message) in messages.iter().enumerate() {
        match peer.send_notification(notification_for(message)).await {
            Ok(()) => {}
            Err(error) => {
                tracing::warn!(
                    %session_uuid,
                    %error,
                    message_uuid = %message.message_uuid,
                    "mike-mcp: claude/channel notification failed; releasing the undelivered mail"
                );
                release_unsent(client, session_uuid, &messages[index..]).await;
                return match error {
                    ServiceError::TransportClosed => BatchOutcome::TransportClosed,
                    _ => BatchOutcome::Delivered,
                };
            }
        }
    }
    BatchOutcome::Delivered
}

pub(crate) async fn release_unsent(
    client: &InspectClient,
    session_uuid: &str,
    unsent: &[ClaimedChannelMessage],
) {
    let uuids: Vec<String> = unsent.iter().map(|m| m.message_uuid.clone()).collect();
    if let Err(error) = client.release_channel_messages(session_uuid, &uuids).await {
        tracing::warn!(
            %session_uuid,
            %error,
            count = uuids.len(),
            "mike-mcp: could not release unsent channel mail; the mailbox reaper will surface it"
        );
    }
}

async fn poll_once(
    peer: &Peer<RoleServer>,
    client: &InspectClient,
    session_uuid: &str,
    state: &mut PollState,
    delivery: &mut Delivery,
) -> Option<Duration> {
    let peer_address = delivery.peer_address();
    let routes = delivery.routes(peer_address.as_deref());
    let response = match client.claim_channel_messages(session_uuid, routes).await {
        Ok(response) => response,
        Err(error) => {
            log_poll_failure(state, session_uuid, &error);
            return Some(state.on_failure());
        }
    };
    let wait = state.on_success();
    if let (true, Some(inbox)) = (response.native_delivery, delivery.inbox.as_ref()) {
        if !push_native_batch(inbox, client, session_uuid, &response.messages).await {
            delivery.inbox = None;
        }
        return delivery.any().then_some(wait);
    }
    if !delivery.channel {
        if !response.messages.is_empty() {
            release_unsent(client, session_uuid, &response.messages).await;
        }
        return Some(wait);
    }
    match push_batch(peer, client, session_uuid, &response.messages).await {
        BatchOutcome::Delivered => Some(wait),
        BatchOutcome::TransportClosed => None,
    }
}

fn log_poll_failure(state: &PollState, session_uuid: &str, error: &crate::error::McpError) {
    if state.should_log_failure() {
        tracing::warn!(
            %session_uuid,
            %error,
            "mike-mcp: channel claim poll failed, backing off and retrying"
        );
    }
}

pub(crate) async fn run_channel_push_loop(
    peer: Peer<RoleServer>,
    client: InspectClient,
    session_uuid: String,
    mut delivery: Delivery,
) {
    log_loop_started(&session_uuid);
    let mut state = PollState::default();
    while let Some(wait) = poll_once(&peer, &client, &session_uuid, &mut state, &mut delivery).await
    {
        tokio::time::sleep(wait).await;
    }
    log_loop_stopped(&session_uuid);
}

fn log_loop_started(session_uuid: &str) {
    tracing::info!(%session_uuid, "mike-mcp: starting MCP-channel push loop");
}

fn log_loop_stopped(session_uuid: &str) {
    tracing::info!(%session_uuid, "mike-mcp: transport closed, stopping the channel push loop");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(
        message_uuid: &str,
        from_session_uuid: Option<&str>,
        from_display_name: Option<&str>,
    ) -> ClaimedChannelMessage {
        message_with_role(message_uuid, from_session_uuid, from_display_name, None)
    }

    fn message_with_role(
        message_uuid: &str,
        from_session_uuid: Option<&str>,
        from_display_name: Option<&str>,
        from_role: Option<&str>,
    ) -> ClaimedChannelMessage {
        ClaimedChannelMessage {
            message_uuid: message_uuid.to_string(),
            from_session_uuid: from_session_uuid.map(str::to_string),
            from_display_name: from_display_name.map(str::to_string),
            from_role: from_role.map(str::to_string),
            envelope: "<system-reminder>hi</system-reminder>".to_string(),
            body: "hi".to_string(),
            from_peer_address: None,
        }
    }

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|a| a.to_string()).collect()
    }

    #[test]
    fn dev_channels_flag_with_mike_entry_opts_in() {
        assert!(argv_opts_into_channel(&args(&[
            "claude",
            "--dangerously-load-development-channels",
            "server:mike",
            "--remote-control",
        ])));
    }

    #[test]
    fn channels_flag_with_mike_among_several_entries_opts_in() {
        assert!(argv_opts_into_channel(&args(&[
            "claude",
            "--channels",
            "plugin:x@y",
            "server:mike",
        ])));
    }

    #[test]
    fn inline_equals_form_opts_in() {
        assert!(argv_opts_into_channel(&args(&[
            "claude",
            "--dangerously-load-development-channels=server:mike",
        ])));
    }

    #[test]
    fn plain_launch_does_not_opt_in() {
        assert!(!argv_opts_into_channel(&args(&[
            "claude",
            "--dangerously-skip-permissions",
            "--remote-control",
        ])));
    }

    #[test]
    fn other_server_entry_does_not_opt_in() {
        assert!(!argv_opts_into_channel(&args(&[
            "claude",
            "--dangerously-load-development-channels",
            "server:other",
        ])));
    }

    #[test]
    fn server_mike_after_an_unrelated_flag_does_not_opt_in() {
        assert!(!argv_opts_into_channel(&args(&[
            "claude",
            "--dangerously-load-development-channels",
            "server:other",
            "--model",
            "server:mike",
        ])));
    }

    #[test]
    fn override_env_values_parse() {
        assert_eq!(override_decision(Some("on")), Some(true));
        assert_eq!(override_decision(Some(" OFF ")), Some(false));
        assert_eq!(override_decision(Some("maybe")), None);
        assert_eq!(override_decision(None), None);
    }

    #[test]
    fn valid_meta_keys_pass() {
        assert!(is_valid_meta_key("message_id"));
        assert!(is_valid_meta_key("_private"));
        assert!(is_valid_meta_key("a"));
    }

    #[test]
    fn invalid_meta_keys_are_rejected() {
        assert!(!is_valid_meta_key(""));
        assert!(!is_valid_meta_key("1abc"));
        assert!(!is_valid_meta_key("has-dash"));
        assert!(!is_valid_meta_key("has space"));
        assert!(!is_valid_meta_key("has.dot"));
    }

    #[test]
    fn build_notification_uses_the_envelope_as_content_verbatim() {
        let msg = message("m-1", Some("s-1"), Some("rust-dev"));
        let (content, _meta) = build_notification(&msg);
        assert_eq!(content, msg.envelope);
    }

    #[test]
    fn build_notification_includes_every_present_field_in_meta() {
        let msg = message("m-1", Some("s-1"), Some("rust-dev"));
        let (_content, meta) = build_notification(&msg);
        assert_eq!(meta.get("message_id"), Some(&"m-1".to_string()));
        assert_eq!(meta.get("from_session"), Some(&"s-1".to_string()));
        assert_eq!(meta.get("from_name"), Some(&"rust-dev".to_string()));
        assert_eq!(meta.get("kind"), Some(&NOTIFICATION_KIND.to_string()));
    }

    #[test]
    fn build_notification_includes_from_role_when_present() {
        let msg = message_with_role("m-3", Some("s-1"), Some("rust-dev"), Some("rust-developer"));
        let (_content, meta) = build_notification(&msg);
        assert_eq!(meta.get("from_role"), Some(&"rust-developer".to_string()));
    }

    #[test]
    fn build_notification_omits_absent_optional_fields() {
        let msg = message("m-2", None, None);
        let (_content, meta) = build_notification(&msg);
        assert!(!meta.contains_key("from_session"));
        assert!(!meta.contains_key("from_name"));
        assert!(meta.contains_key("message_id"));
    }

    #[test]
    fn poll_state_resets_backoff_on_success() {
        let mut state = PollState::default();
        state.on_failure();
        state.on_failure();
        let wait = state.on_success();
        assert_eq!(wait, POLL_INTERVAL);
    }

    #[test]
    fn poll_state_backs_off_exponentially_up_to_the_cap() {
        let mut state = PollState::default();
        let first = state.on_failure();
        let second = state.on_failure();
        let third = state.on_failure();
        let fourth = state.on_failure();
        assert!(first < second);
        assert!(second < third);
        assert!(third <= MAX_BACKOFF);
        assert!(fourth <= MAX_BACKOFF);
    }

    #[test]
    fn poll_state_logs_only_the_first_failure_of_an_outage() {
        let mut state = PollState::default();
        assert!(state.should_log_failure());
        state.on_failure();
        assert!(!state.should_log_failure());
        state.on_failure();
        assert!(!state.should_log_failure());
        state.on_success();
        assert!(state.should_log_failure());
    }
}
