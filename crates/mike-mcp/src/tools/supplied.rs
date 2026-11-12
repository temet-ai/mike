use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde_json::{Map, Value, json};

use super::{object_schema, wrap_success};
use crate::client::InspectClient;
use crate::error::McpError;

pub const TOOL_LIST_SUPPLIED_ITEMS: &str = "list_supplied_items";

pub const SUPPLIED_TOOL_NAMES: &[&str] = &[TOOL_LIST_SUPPLIED_ITEMS];

pub fn owns(name: &str) -> bool {
    SUPPLIED_TOOL_NAMES.contains(&name)
}

pub fn supplied_tools() -> Vec<Tool> {
    vec![Tool::new(
        Cow::Borrowed(TOOL_LIST_SUPPLIED_ITEMS),
        Cow::Borrowed(
            "List every lesson and work-thread card mike supplied with a request (`request_id`) \
             or across a session (`session_id`), oldest first: the item and its full text, the \
             route that delivered it (proxy, mod, tether), the score that chose it, its chance \
             of being supplied (1 for a lesson; the keeper arm share for a card), and the \
             overnight verdict (used, corrected_anyway, ignored) with its rationale once judged. \
             Paged with `limit` and `offset`; `has_more` and `next_offset` say whether more rows \
             wait. Use it to answer what memory a turn was given and whether it helped.",
        ),
        object_schema(
            vec![
                (
                    "request_id",
                    json!({"type": "integer", "description": "The request the items were supplied with."}),
                ),
                (
                    "session_id",
                    json!({"type": "string", "description": "A session UUID: every item supplied in that session."}),
                ),
                (
                    "limit",
                    json!({"type": "integer", "minimum": 1, "maximum": 500, "description": "Rows per page, default 100."}),
                ),
                (
                    "offset",
                    json!({"type": "integer", "minimum": 0, "description": "Rows to skip; pass the previous answer's next_offset."}),
                ),
            ],
            &[],
        ),
    )]
}

fn pairs(arguments: Option<&Map<String, Value>>) -> Vec<(&'static str, String)> {
    let Some(a) = arguments else {
        return Vec::new();
    };
    ["request_id", "session_id", "limit", "offset"]
        .iter()
        .filter_map(|k| {
            let value = match a.get(*k)? {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                _ => return None,
            };
            Some((*k, value))
        })
        .collect()
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    if name != TOOL_LIST_SUPPLIED_ITEMS {
        return Err(RmcpError::invalid_params(
            format!("unknown supplied-items tool: {name}"),
            None,
        ));
    }
    let query = pairs(arguments.as_ref());
    if !query
        .iter()
        .any(|(k, _)| *k == "request_id" || *k == "session_id")
    {
        return Err(RmcpError::invalid_params(
            "list_supplied_items needs `request_id` (a request id) or `session_id` (a session UUID); neither was given".to_owned(),
            None,
        ));
    }
    let value = client
        .supplied_items_get(&query)
        .await
        .map_err(McpError::into_rmcp)?;
    Ok(wrap_success("supplied", value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supplied_tools_serve_every_name() {
        assert_eq!(supplied_tools().len(), SUPPLIED_TOOL_NAMES.len());
        assert!(owns(TOOL_LIST_SUPPLIED_ITEMS));
    }

    #[test]
    fn pairs_keep_only_the_query_keys() {
        let mut args = Map::new();
        args.insert("request_id".into(), json!(42));
        args.insert("select".into(), json!(["/x"]));
        assert_eq!(pairs(Some(&args)), vec![("request_id", "42".to_owned())]);
    }
}
