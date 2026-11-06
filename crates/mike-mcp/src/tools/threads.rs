use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{object_schema, parse_args_required, wrap_success};
use crate::client::InspectClient;
use crate::error::McpError;

pub const TOOL_READ_THREAD: &str = "read_thread";
pub const TOOL_RESOLVE_THREAD: &str = "resolve_thread";

pub const THREAD_TOOL_NAMES: &[&str] = &[TOOL_READ_THREAD, TOOL_RESOLVE_THREAD];

pub fn owns(name: &str) -> bool {
    THREAD_TOOL_NAMES.contains(&name)
}

pub fn thread_tools() -> Vec<Tool> {
    vec![read_tool(), resolve_tool()]
}

fn thread_id_property() -> Value {
    json!({"type": "string", "description": "The thread id, from get_session_threads or from a send_session_message answer. A fleet's comms thread has the fleet's id."})
}

fn read_tool() -> Tool {
    Tool::new(
        Cow::Borrowed(TOOL_READ_THREAD),
        Cow::Borrowed(
            "Read one mail thread whole: its summary (subject, tags, participants, resolved_at) \
             and every message in order with sender, recipient, queue time and delivery state. \
             Read-only. Use it to see what a thread settled before you answer in it or resolve \
             it; find thread ids with get_session_threads. Refused: an unknown thread id.",
        ),
        object_schema(vec![("thread_id", thread_id_property())], &["thread_id"]),
    )
}

fn resolve_tool() -> Tool {
    Tool::new(
        Cow::Borrowed(TOOL_RESOLVE_THREAD),
        Cow::Borrowed(
            "Mark a mail thread resolved once what it was opened for is settled, so it leaves \
             the open threads. Returns the thread id and `resolved_at`. Calling it again on a \
             resolved thread changes nothing. Refused: an unknown thread id. Read the thread \
             with read_thread first.",
        ),
        object_schema(vec![("thread_id", thread_id_property())], &["thread_id"]),
    )
}

#[derive(Debug, Deserialize)]
struct ThreadArgs {
    thread_id: String,
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    let args = parse_args_required::<ThreadArgs>(arguments)?;
    let thread_id = args.thread_id.trim();
    let (label, outcome) = match name {
        TOOL_READ_THREAD => ("thread", client.thread_read(thread_id).await),
        TOOL_RESOLVE_THREAD => ("resolved", client.thread_resolve(thread_id).await),
        other => {
            return Err(RmcpError::invalid_params(
                format!("unknown thread tool: {other}"),
                None,
            ));
        }
    };
    Ok(wrap_success(label, outcome.map_err(McpError::into_rmcp)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thread_tools_serve_every_name_with_descriptions_under_the_limit() {
        let tools = thread_tools();
        assert_eq!(tools.len(), THREAD_TOOL_NAMES.len());
        for tool in &tools {
            assert!(owns(tool.name.as_ref()));
            let length = tool.description.as_deref().map_or(0, str::len);
            assert!(length > 0 && length < 2048, "{}: {length}", tool.name);
        }
    }
}
