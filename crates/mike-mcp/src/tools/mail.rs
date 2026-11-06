mod reply_identity;
mod schemas;

use rmcp::ErrorData as RmcpError;
use rmcp::model::{CallToolResult, Tool};
use serde::Deserialize;
use serde_json::{Map, Value};

use super::{parse_args_required, wrap_success};
use crate::client::InspectClient;
use crate::error::McpError;

pub const TOOL_SEND_SESSION_MESSAGE: &str = "send_session_message";
pub const TOOL_GET_SESSION_MESSAGES: &str = "get_session_messages";
pub const TOOL_ACK_SESSION_MESSAGE: &str = "ack_session_message";
pub const TOOL_VERIFY_SESSION_MESSAGE: &str = "verify_session_message";
pub const TOOL_VERIFY_MESSAGE_READ: &str = "verify_message_read";

pub const MAIL_TOOL_NAMES: &[&str] = &[
    TOOL_SEND_SESSION_MESSAGE,
    TOOL_GET_SESSION_MESSAGES,
    TOOL_ACK_SESSION_MESSAGE,
    TOOL_VERIFY_SESSION_MESSAGE,
    TOOL_VERIFY_MESSAGE_READ,
];

pub fn mail_tools() -> Vec<Tool> {
    vec![
        schemas::send_session_message_tool(),
        schemas::get_session_messages_tool(),
        schemas::ack_session_message_tool(),
        schemas::verify_session_message_tool(),
        schemas::verify_message_read_tool(),
    ]
}

pub fn owns(name: &str) -> bool {
    MAIL_TOOL_NAMES.contains(&name)
}

fn resolve_send_target<'a>(
    to: Option<&'a str>,
    thread_id: Option<&'a str>,
    subject: Option<&'a str>,
) -> Result<crate::client::SendTarget<'a>, &'static str> {
    match (to, thread_id, subject) {
        (Some(to), ..) => Ok(crate::client::SendTarget::Recipient(to)),
        (None, Some(thread_id), _) => Ok(crate::client::SendTarget::ThreadPost(thread_id)),
        (None, None, Some(subject)) => Ok(crate::client::SendTarget::OpenThread(subject)),
        (None, None, None) => Err(
            "`to` is required unless `subject` (open a thread) or `thread_id` (post into one) \
             is supplied",
        ),
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct SendSessionMessageArgs {
    #[serde(default)]
    pub(crate) to: Option<String>,
    pub(crate) message: String,
    #[serde(default)]
    pub(crate) from: Option<String>,
    #[serde(default)]
    pub(crate) request_reply: Option<bool>,
    #[serde(default)]
    pub(crate) thread_id: Option<String>,
    #[serde(default)]
    pub(crate) in_reply_to: Option<String>,
    #[serde(default)]
    pub(crate) subject: Option<String>,
    #[serde(default)]
    pub(crate) tags: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) context: Option<Vec<Value>>,
}

fn attach_from_unverified(mut value: Value, unverified: bool) -> Value {
    if let Value::Object(map) = &mut value
        && unverified
    {
        map.insert("from_unverified".to_string(), Value::Bool(true));
    }
    value
}

#[derive(Debug, Deserialize)]
pub(crate) struct GetSessionMessagesArgs {
    pub(crate) session_id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MailboxMessageArgs {
    pub(crate) session_id: String,
    pub(crate) message_uuid: String,
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    match name {
        TOOL_SEND_SESSION_MESSAGE => {
            let args = parse_args_required::<SendSessionMessageArgs>(arguments)?;
            let target = resolve_send_target(
                args.to.as_deref(),
                args.thread_id.as_deref(),
                args.subject.as_deref(),
            )
            .map_err(|msg| RmcpError::invalid_params(msg, None))?;
            let resolution = reply_identity::resolve_from(
                client,
                &args.message,
                args.from.as_deref(),
                args.request_reply.unwrap_or(false),
            )
            .await?;
            let value = client
                .send_session_message(
                    target,
                    &args.message,
                    crate::client::SendMessageOptions {
                        from: resolution.from.as_deref(),
                        thread_id: args.thread_id.as_deref(),
                        in_reply_to: args.in_reply_to.as_deref(),
                        subject: args.subject.as_deref(),
                        tags: args.tags.as_deref(),
                        context: args.context.as_deref(),
                    },
                )
                .await
                .map_err(McpError::into_rmcp)?;
            let value = attach_from_unverified(value, resolution.from_unverified);
            Ok(wrap_success("message", value))
        }
        TOOL_GET_SESSION_MESSAGES => {
            let args = parse_args_required::<GetSessionMessagesArgs>(arguments)?;
            let value = client
                .get_session_messages(&args.session_id)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("mailbox", value))
        }
        TOOL_ACK_SESSION_MESSAGE => {
            let args = parse_args_required::<MailboxMessageArgs>(arguments)?;
            let value = client
                .ack_session_message(&args.session_id, &args.message_uuid)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("ack", value))
        }
        TOOL_VERIFY_SESSION_MESSAGE => {
            let args = parse_args_required::<MailboxMessageArgs>(arguments)?;
            let value = client
                .verify_session_message(&args.session_id, &args.message_uuid)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("message", value))
        }
        TOOL_VERIFY_MESSAGE_READ => {
            let args = parse_args_required::<MailboxMessageArgs>(arguments)?;
            let value = client
                .verify_message_read(&args.session_id, &args.message_uuid)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("receipt", value))
        }
        other => Err(RmcpError::invalid_params(
            format!("unknown mail tool: {other}"),
            None,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mail_tool_names_match_the_built_tools() {
        let built: Vec<String> = mail_tools()
            .into_iter()
            .map(|t| t.name.to_string())
            .collect();
        let mut names: Vec<String> = MAIL_TOOL_NAMES.iter().map(|s| s.to_string()).collect();
        let mut built_sorted = built.clone();
        names.sort();
        built_sorted.sort();
        assert_eq!(names, built_sorted);
        assert!(built.iter().all(|n| owns(n)));
    }

    #[test]
    fn mail_domain_carries_the_receive_half() {
        assert!(owns(TOOL_GET_SESSION_MESSAGES));
        assert!(owns(TOOL_ACK_SESSION_MESSAGE));
        assert!(owns(TOOL_VERIFY_SESSION_MESSAGE));
    }

    #[test]
    fn mail_domain_carries_the_sender_receipt() {
        assert!(owns(TOOL_VERIFY_MESSAGE_READ));
    }

    #[test]
    fn resolve_send_target_to_present_wins() {
        assert_eq!(
            resolve_send_target(Some("sess-1"), Some("thread-1"), Some("subject")),
            Ok(crate::client::SendTarget::Recipient("sess-1"))
        );
        assert_eq!(
            resolve_send_target(Some("sess-1"), None, None),
            Ok(crate::client::SendTarget::Recipient("sess-1"))
        );
    }

    #[test]
    fn resolve_send_target_to_absent_thread_id_wins_over_subject() {
        assert_eq!(
            resolve_send_target(None, Some("thread-1"), Some("subject")),
            Ok(crate::client::SendTarget::ThreadPost("thread-1"))
        );
        assert_eq!(
            resolve_send_target(None, Some("thread-1"), None),
            Ok(crate::client::SendTarget::ThreadPost("thread-1"))
        );
    }

    #[test]
    fn resolve_send_target_to_and_thread_id_absent_falls_back_to_subject() {
        assert_eq!(
            resolve_send_target(None, None, Some("open a thread")),
            Ok(crate::client::SendTarget::OpenThread("open a thread"))
        );
    }

    #[test]
    fn resolve_send_target_all_absent_is_rejected() {
        assert!(resolve_send_target(None, None, None).is_err());
    }

    #[test]
    fn attach_from_unverified_adds_the_flag_only_when_true() {
        let base = serde_json::json!({"message_uuid": "m-1"});
        assert_eq!(
            attach_from_unverified(base.clone(), false),
            base,
            "a verified send must not gain the flag"
        );
        let flagged = attach_from_unverified(base, true);
        assert_eq!(flagged["from_unverified"], serde_json::json!(true));
    }

    #[test]
    fn send_session_message_schema_does_not_require_to() {
        let tool = schemas::send_session_message_tool();
        let Some(required) = tool.input_schema.get("required").and_then(Value::as_array) else {
            panic!("schema must declare a required array");
        };
        let required: Vec<&str> = required.iter().filter_map(Value::as_str).collect();
        assert_eq!(
            required,
            vec!["message"],
            "`to` must not be required — AC17 makes it conditional on subject/thread_id"
        );
    }
}
