use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{
    id_only_schema, id_with_pointer_schema, object_schema, order_property, parse_args_default,
    parse_args_required, wrap_success, wrap_success_paged,
};
use crate::client::InspectClient;
use crate::error::McpError;

pub const REQUESTS_TOOL_NAMES: &[&str] = &[
    TOOL_LIST_REQUESTS,
    TOOL_GET_REQUEST,
    TOOL_GET_REQUEST_SUMMARY,
    TOOL_GET_REQUEST_BODY,
    TOOL_GET_REQUEST_SENT_BODY,
    TOOL_GET_REQUEST_TRANSFORMATION_MANIFEST,
    TOOL_GET_RESPONSE_BODY,
    TOOL_GET_ACCUMULATED_TEXT,
    TOOL_GET_REQUEST_STREAM_CHUNKS,
    TOOL_GET_REQUEST_TOOL_CALLS,
];

pub const TOOL_LIST_REQUESTS: &str = "list_requests";
pub const TOOL_GET_REQUEST: &str = "get_request";
pub const TOOL_GET_REQUEST_SUMMARY: &str = "get_request_summary";
pub const TOOL_GET_REQUEST_BODY: &str = "get_request_body";
pub const TOOL_GET_REQUEST_SENT_BODY: &str = "get_request_sent_body";
pub const TOOL_GET_REQUEST_TRANSFORMATION_MANIFEST: &str = "get_request_transformation_manifest";
pub const TOOL_GET_RESPONSE_BODY: &str = "get_response_body";
pub const TOOL_GET_ACCUMULATED_TEXT: &str = "get_accumulated_text";
pub const TOOL_GET_REQUEST_STREAM_CHUNKS: &str = "get_request_stream_chunks";
pub const TOOL_GET_REQUEST_TOOL_CALLS: &str = "get_request_tool_calls";

pub fn requests_tools() -> Vec<Tool> {
    vec![
        list_requests_tool(),
        get_request_tool(),
        get_request_summary_tool(),
        get_request_body_tool(),
        get_request_sent_body_tool(),
        get_request_transformation_manifest_tool(),
        get_response_body_tool(),
        get_accumulated_text_tool(),
        get_request_stream_chunks_tool(),
        get_request_tool_calls_tool(),
    ]
}

pub fn owns(name: &str) -> bool {
    REQUESTS_TOOL_NAMES.contains(&name)
}

fn list_requests_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "offset",
                json!({
                    "type": "integer",
                    "minimum": 0,
                    "description": "Skip this many rows before returning results."
                }),
            ),
            (
                "limit",
                json!({
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 200,
                    "description": "Maximum number of rows to return (default 50, max 200)."
                }),
            ),
            (
                "cursor",
                json!({
                    "type": "string",
                    "description": "Opaque keyset cursor: the `next_cursor` of the previous page. When set, `offset` is ignored and the page continues after that row."
                }),
            ),
            ("order", order_property()),
            (
                "date_from",
                json!({
                    "type": "string",
                    "description": "RFC 3339 lower bound on `created_at` (inclusive). Absent → 7-day default window."
                }),
            ),
            (
                "date_to",
                json!({
                    "type": "string",
                    "description": "RFC 3339 upper bound on `created_at` (exclusive). Absent → now (paired with 7-day default)."
                }),
            ),
        ],
        &[],
    );
    Tool::new(
        Cow::Borrowed(TOOL_LIST_REQUESTS),
        Cow::Borrowed(
            "List stored Claude API requests, newest first by default. Each row: id, method, \
             path, status, model, human_prompt, tool_names, timestamps. Full request: \
             `get_request`. Response text: `get_response_body`.",
        ),
        schema,
    )
}

fn get_request_tool() -> Tool {
    let schema = object_schema(
        vec![(
            "id",
            json!({
                "type": "integer",
                "minimum": 0,
                "description": "The numeric request id."
            }),
        )],
        &["id"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_GET_REQUEST),
        Cow::Borrowed(
            "Fetch a stored Claude API request by id. \
             Includes metadata, headers, params, SSE accumulators, diagnostics.",
        ),
        schema,
    )
}

fn get_request_summary_tool() -> Tool {
    Tool::new(
        Cow::Borrowed(TOOL_GET_REQUEST_SUMMARY),
        Cow::Borrowed(
            "List-row projection of a single request (id, method, path, status, \
             human_prompt, tool_names). Cheaper than `get_request`; omits headers, \
             params, SSE accumulators, response bodies, diagnostics.",
        ),
        id_only_schema(),
    )
}

fn get_request_body_tool() -> Tool {
    let mut schema = id_with_pointer_schema();
    std::sync::Arc::make_mut(&mut schema).remove("required");
    if let Some(Value::Object(properties)) =
        std::sync::Arc::make_mut(&mut schema).get_mut("properties")
    {
        properties.insert("tool_use_id".into(), json!({"type":"string", "description":"Recover a captured tool result; supply sha256 too, without pointer."}));
        properties.insert("sha256".into(), json!({"type":"string", "pattern":"^[a-fA-F0-9]{64}$", "description":"Expected SHA-256 of the decoded original result text; mismatch refuses recovery."}));
    }
    Tool::new(
        Cow::Borrowed(TOOL_GET_REQUEST_BODY),
        Cow::Borrowed(
            "Raw request body the client sent (/v1/messages JSON — full system, messages, tools). \
             Returns {text, byte_size, has_body}. \
             Optional `pointer` (RFC 6901 JSON pointer) projects to a subtree. \
             For exact compressed-result recovery, pass the marker's tool_use_id and sha256 \
             (no id needed); text is the JSON-encoded original string, unaffected by later file edits.",
        ),
        schema,
    )
}

fn get_request_sent_body_tool() -> Tool {
    Tool::new(
        Cow::Borrowed(TOOL_GET_REQUEST_SENT_BODY),
        Cow::Borrowed(
            "Body actually forwarded upstream after filter ran. \
             Differs from `get_request_body` when filter rewrites `system`. \
             Same pointer projection.",
        ),
        id_with_pointer_schema(),
    )
}

fn get_request_transformation_manifest_tool() -> Tool {
    Tool::new(
        Cow::Borrowed(TOOL_GET_REQUEST_TRANSFORMATION_MANIFEST),
        Cow::Borrowed(
            "How the client's original request body became the bytes forwarded upstream: \
             schema_version, original and sent SHA256, per-stage outcomes and targets. \
             Returns {request_id, has_manifest, manifest}; has_manifest is false when no \
             manifest was recorded for this request. Same optional `pointer` projection as \
             `get_request_body`.",
        ),
        id_with_pointer_schema(),
    )
}

fn get_response_body_tool() -> Tool {
    Tool::new(
        Cow::Borrowed(TOOL_GET_RESPONSE_BODY),
        Cow::Borrowed(
            "Raw response body (or accumulated SSE as newline-joined events). \
             Same shape and pointer projection as `get_request_body`.",
        ),
        id_with_pointer_schema(),
    )
}

fn get_accumulated_text_tool() -> Tool {
    Tool::new(
        Cow::Borrowed(TOOL_GET_ACCUMULATED_TEXT),
        Cow::Borrowed(
            "Full reconstructed text response for a streaming request \
             (all text blocks concatenated). \
             For individual blocks including thinking/tool_use use `get_request_stream_chunks`.",
        ),
        id_only_schema(),
    )
}

fn get_request_stream_chunks_tool() -> Tool {
    Tool::new(
        Cow::Borrowed(TOOL_GET_REQUEST_STREAM_CHUNKS),
        Cow::Borrowed(
            "Content-block rows for a streaming request; consecutive same-type SSE deltas are \
             one block. Each row: seq_ord, content, byte_size, content_sha256, \
             block_type (text|thinking|tool_use), signature (thinking blocks), \
             wire_tool_use_id (tool_use blocks), arrived_at. \
             `get_accumulated_text` returns the concatenated text only.",
        ),
        id_only_schema(),
    )
}

fn get_request_tool_calls_tool() -> Tool {
    Tool::new(
        Cow::Borrowed(TOOL_GET_REQUEST_TOOL_CALLS),
        Cow::Borrowed(
            "All tool calls emitted by request `id`, each paired with its result. \
             Full input and result content, never sliced. Ordered by emission order (ord). \
             result is null when the call was never answered.",
        ),
        id_only_schema(),
    )
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct ListRequestsArgs {
    pub(crate) offset: Option<u64>,
    pub(crate) limit: Option<u64>,
    #[serde(default)]
    pub(crate) cursor: Option<String>,
    #[serde(default)]
    pub(crate) order: Option<String>,
    #[serde(default)]
    pub(crate) date_from: Option<String>,
    #[serde(default)]
    pub(crate) date_to: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GetRequestArgs {
    pub(crate) id: u64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GetBodyArgs {
    pub(crate) id: u64,
    #[serde(default)]
    pub(crate) pointer: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GetOriginalBodyArgs {
    #[serde(default)]
    id: Option<u64>,
    pointer: Option<String>,
    tool_use_id: Option<String>,
    sha256: Option<String>,
}

impl GetOriginalBodyArgs {
    fn query(&self) -> Result<Vec<(&'static str, String)>, RmcpError> {
        match (&self.tool_use_id, &self.sha256) {
            (None, None) => Ok(self
                .pointer
                .as_ref()
                .map(|p| vec![("pointer", p.clone())])
                .unwrap_or_default()),
            (Some(id), Some(hash))
                if self.pointer.is_none()
                    && !id.is_empty()
                    && id
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
                    && hash.len() == 64
                    && hash.bytes().all(|c| c.is_ascii_hexdigit()) =>
            {
                Ok(vec![("tool_use_id", id.clone()), ("sha256", hash.clone())])
            }
            _ => Err(RmcpError::invalid_params(
                "Recovery requires tool_use_id and a SHA-256, without pointer",
                None,
            )),
        }
    }

    async fn fetch(&self, client: &InspectClient) -> Result<Value, RmcpError> {
        let query = self.query()?;
        let value = match (&self.tool_use_id, self.id) {
            (Some(tool_use_id), _) => {
                client
                    .get_json_with_not_found_query(
                        &format!("/api/tool-results/{tool_use_id}/original"),
                        &query,
                        &format!("tool result {tool_use_id}"),
                    )
                    .await
            }
            (None, Some(id)) => client.get_request_body(id, self.pointer.as_deref()).await,
            (None, None) => {
                return Err(RmcpError::invalid_params(
                    "get_request_body needs id, or tool_use_id with sha256",
                    None,
                ));
            }
        };
        value.map_err(McpError::into_rmcp)
    }
}

fn list_requests_query(args: &ListRequestsArgs) -> Vec<(&'static str, String)> {
    let mut query: Vec<(&'static str, String)> = Vec::new();
    if let Some(o) = args.offset {
        query.push(("offset", o.to_string()));
    }
    if let Some(l) = args.limit {
        query.push(("limit", l.to_string()));
    }
    if let Some(ref cursor) = args.cursor {
        query.push(("cursor", cursor.clone()));
    }
    if let Some(ref ord) = args.order {
        query.push(("order", ord.clone()));
    }
    if let Some(ref df) = args.date_from {
        query.push(("date_from", df.clone()));
    }
    if let Some(ref dt) = args.date_to {
        query.push(("date_to", dt.clone()));
    }
    query
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    match name {
        TOOL_LIST_REQUESTS => {
            let args = parse_args_default::<ListRequestsArgs>(arguments)?;
            let query = list_requests_query(&args);
            let page = client
                .get_json_paged("/api/requests", &query)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success_paged(
                "requests",
                page.body,
                page.window_note,
                page.next_cursor,
            ))
        }
        TOOL_GET_REQUEST => {
            let args = parse_args_required::<GetRequestArgs>(arguments)?;
            let value = client
                .get_request(args.id)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("request", value))
        }
        TOOL_GET_REQUEST_SUMMARY => {
            let args = parse_args_required::<GetRequestArgs>(arguments)?;
            let value = client
                .get_request_summary(args.id)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("summary", value))
        }
        TOOL_GET_REQUEST_BODY => {
            let args = parse_args_required::<GetOriginalBodyArgs>(arguments)?;
            let value = args.fetch(client).await?;
            Ok(wrap_success("body", value))
        }
        TOOL_GET_REQUEST_SENT_BODY => {
            let args = parse_args_required::<GetBodyArgs>(arguments)?;
            let value = client
                .get_request_sent_body(args.id, args.pointer.as_deref())
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("body", value))
        }
        TOOL_GET_REQUEST_TRANSFORMATION_MANIFEST => {
            let args = parse_args_required::<GetBodyArgs>(arguments)?;
            let value = client
                .get_request_transformation_manifest(args.id, args.pointer.as_deref())
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("manifest", value))
        }
        TOOL_GET_RESPONSE_BODY => {
            let args = parse_args_required::<GetBodyArgs>(arguments)?;
            let value = client
                .get_response_body(args.id, args.pointer.as_deref())
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("body", value))
        }
        TOOL_GET_ACCUMULATED_TEXT => {
            let args = parse_args_required::<GetRequestArgs>(arguments)?;
            let value = client
                .get_accumulated_text(args.id)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("accumulated_text", value))
        }
        TOOL_GET_REQUEST_STREAM_CHUNKS => {
            let args = parse_args_required::<GetRequestArgs>(arguments)?;
            let value = client
                .get_request_stream_chunks(args.id)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("stream_chunks", value))
        }
        TOOL_GET_REQUEST_TOOL_CALLS => {
            let args = parse_args_required::<GetRequestArgs>(arguments)?;
            let value = client
                .get_request_tool_calls(args.id)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("tool_calls", value))
        }
        other => Err(RmcpError::invalid_params(
            format!("unknown request tool: {other}"),
            None,
        )),
    }
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn recovery_arguments_without_id_reach_the_tool_result_route() -> anyhow::Result<()> {
        let server = MockServer::start().await;
        let client = InspectClient::new(server.uri());
        let hash = "a".repeat(64);
        Mock::given(method("GET"))
            .and(path("/api/tool-results/tool-one/original"))
            .and(query_param("tool_use_id", "tool-one"))
            .and(query_param("sha256", hash.as_str()))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"text":"\"original\"","has_body":true})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let args = json!({"tool_use_id":"tool-one","sha256":hash});
        call_tool(&client, TOOL_GET_REQUEST_BODY, args.as_object().cloned()).await?;
        Ok(())
    }

    #[test]
    fn recovery_rejects_partial_and_conflicting_selectors() -> anyhow::Result<()> {
        for value in [
            json!({"id":1,"tool_use_id":"x"}),
            json!({"id":1,"tool_use_id":"x","sha256":"bad"}),
            json!({"id":1,"tool_use_id":"x","sha256":"a".repeat(64),"pointer":"/messages"}),
        ] {
            let args: GetOriginalBodyArgs = serde_json::from_value(value)?;
            assert!(args.query().is_err());
        }
        Ok(())
    }
}
