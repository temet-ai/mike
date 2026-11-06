use serde_json::Value;

use super::InspectClient;
use super::{SendMessageOptions, SendTarget};
use crate::error::McpError;

pub(crate) fn path_segment(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.trim().bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b':' | b'~' => {
                out.push(char::from(byte));
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

impl InspectClient {
    pub(crate) async fn send_session_message(
        &self,
        target: SendTarget<'_>,
        message: &str,
        options: SendMessageOptions<'_>,
    ) -> Result<Value, McpError> {
        match target {
            SendTarget::Recipient(to) => self.send_to_recipient(to, message, &options).await,
            SendTarget::ThreadPost(thread_id) => {
                self.post_to_thread(thread_id, message, &options).await
            }
            SendTarget::OpenThread(subject) => self.open_thread(subject, message, &options).await,
        }
    }

    async fn send_to_recipient(
        &self,
        to: &str,
        message: &str,
        options: &SendMessageOptions<'_>,
    ) -> Result<Value, McpError> {
        let path = format!("/api/sessions/{}/message", path_segment(to));
        let mut body = serde_json::Map::new();
        body.insert("message".to_string(), Value::String(message.to_string()));
        if let Some(from) = options.from {
            body.insert(
                "from_session_id".to_string(),
                Value::String(from.to_string()),
            );
        }
        if let Some(thread_id) = options.thread_id {
            body.insert(
                "thread_id".to_string(),
                Value::String(thread_id.to_string()),
            );
        }
        if let Some(in_reply_to) = options.in_reply_to {
            body.insert(
                "in_reply_to".to_string(),
                Value::String(in_reply_to.to_string()),
            );
        }
        if let Some(subject) = options.subject {
            body.insert("subject".to_string(), Value::String(subject.to_string()));
        }
        if let Some(tags) = options.tags {
            body.insert(
                "tags".to_string(),
                Value::Array(tags.iter().cloned().map(Value::String).collect()),
            );
        }
        if let Some(context) = options.context {
            body.insert("context".to_string(), Value::Array(context.to_vec()));
        }
        self.post_json(&path, &Value::Object(body)).await
    }

    async fn post_to_thread(
        &self,
        thread_id: &str,
        message: &str,
        options: &SendMessageOptions<'_>,
    ) -> Result<Value, McpError> {
        let path = format!("/api/threads/{thread_id}/message");
        let mut body = serde_json::Map::new();
        body.insert("body".to_string(), Value::String(message.to_string()));
        body.insert("to".to_string(), Value::Array(Vec::new()));
        if let Some(from) = options.from {
            body.insert(
                "from_session_id".to_string(),
                Value::String(from.to_string()),
            );
        }
        if let Some(in_reply_to) = options.in_reply_to {
            body.insert(
                "in_reply_to".to_string(),
                Value::String(in_reply_to.to_string()),
            );
        }
        if let Some(context) = options.context {
            body.insert("context".to_string(), Value::Array(context.to_vec()));
        }
        self.post_json(&path, &Value::Object(body)).await
    }

    async fn open_thread(
        &self,
        subject: &str,
        message: &str,
        options: &SendMessageOptions<'_>,
    ) -> Result<Value, McpError> {
        let mut body = serde_json::Map::new();
        body.insert("subject".to_string(), Value::String(subject.to_string()));
        body.insert("body".to_string(), Value::String(message.to_string()));
        if let Some(from) = options.from {
            body.insert(
                "from_session_id".to_string(),
                Value::String(from.to_string()),
            );
        }
        if let Some(tags) = options.tags {
            body.insert(
                "tags".to_string(),
                Value::Array(tags.iter().cloned().map(Value::String).collect()),
            );
        }
        self.post_json("/api/threads", &Value::Object(body)).await
    }

    pub(crate) async fn get_session_messages(&self, session_id: &str) -> Result<Value, McpError> {
        let path = format!("/api/sessions/{session_id}/messages");
        self.get_json_with_not_found(&path, "session").await
    }

    pub(crate) async fn ack_session_message(
        &self,
        session_id: &str,
        message_uuid: &str,
    ) -> Result<Value, McpError> {
        let path = format!("/api/sessions/{session_id}/messages/{message_uuid}/ack");
        self.post_json_with_not_found(
            &path,
            &Value::Object(serde_json::Map::new()),
            "mailbox message addressed to this session",
        )
        .await
    }

    pub(crate) async fn verify_session_message(
        &self,
        session_id: &str,
        message_uuid: &str,
    ) -> Result<Value, McpError> {
        let path = format!("/api/sessions/{session_id}/messages/{message_uuid}");
        self.get_json_with_not_found(&path, "mailbox message addressed to this session")
            .await
    }

    pub(crate) async fn verify_message_read(
        &self,
        session_id: &str,
        message_uuid: &str,
    ) -> Result<Value, McpError> {
        let path = format!("/api/sessions/{session_id}/messages/{message_uuid}/receipt");
        self.get_json_with_not_found(&path, "mailbox message sent by this session")
            .await
    }
}
