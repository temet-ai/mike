use std::collections::HashSet;

use mike::api::manifest::{Coverage, inspect_route_manifest};
use mike_mcp::resources::{
    URI_REQUESTS, URI_REQUESTS_PREFIX, URI_SESSIONS, URI_SESSIONS_PREFIX, URI_STATUS,
    list_resources,
};
use mike_mcp::tools::all_tools;

fn known_resource_uri(uri: &str) -> bool {
    if uri == URI_STATUS || uri == URI_REQUESTS || uri == URI_SESSIONS {
        return true;
    }
    if uri.starts_with(URI_REQUESTS_PREFIX) || uri.starts_with(URI_SESSIONS_PREFIX) {
        return true;
    }
    list_resources().iter().any(|r| r.uri == uri)
}

#[test]
fn every_inspect_route_has_mcp_coverage() {
    let tools_vec = all_tools();
    let tools: HashSet<&str> = tools_vec.iter().map(|t| t.name.as_ref()).collect();
    for entry in inspect_route_manifest() {
        match entry.coverage {
            Coverage::Tool(name) => assert!(
                tools.contains(name),
                "route {} {} declares Coverage::Tool({name:?}) but no such tool exists \
                 in mike_mcp::tools::all_tools(). Either add the tool or change the \
                 manifest entry.",
                entry.method,
                entry.path,
            ),
            Coverage::AggregatedInto(name) => assert!(
                tools.contains(name),
                "route {} {} declares Coverage::AggregatedInto({name:?}) but the \
                 aggregate tool does not exist in mike_mcp::tools::all_tools().",
                entry.method,
                entry.path,
            ),
            Coverage::Resource(uri) => assert!(
                known_resource_uri(uri),
                "route {} {} declares Coverage::Resource({uri:?}) but that URI is not \
                 listed in mike_mcp::resources::list_resources() and does not match any \
                 dynamic prefix.",
                entry.method,
                entry.path,
            ),
            Coverage::Exempt { reason } => assert!(
                !reason.trim().is_empty(),
                "route {} {} is Exempt but has an empty reason. Every exemption must \
                 document WHY the route is excluded from MCP coverage.",
                entry.method,
                entry.path,
            ),
        }
    }
}

#[test]
fn every_mcp_tool_has_a_backing_route() {
    let declared: HashSet<&str> = inspect_route_manifest()
        .iter()
        .filter_map(|e| match e.coverage {
            Coverage::Tool(n) | Coverage::AggregatedInto(n) => Some(n),
            _ => None,
        })
        .collect();
    for tool in all_tools() {
        assert!(
            declared.contains(tool.name.as_ref()),
            "MCP tool {} has no manifest entry pointing at it. Add a Coverage::Tool \
             or Coverage::AggregatedInto({:?}) entry in mike::api::manifest.",
            tool.name,
            tool.name.as_ref(),
        );
    }
}

#[test]
fn no_tool_description_mentions_obsolete_hash_prefixed_session_id() -> anyhow::Result<()> {
    for tool in all_tools() {
        let schema = serde_json::to_value(tool.input_schema.as_ref().clone())?;
        let Some(props) = schema.get("properties").and_then(|p| p.as_object()) else {
            continue;
        };
        for (param_name, param_schema) in props {
            let Some(description) = param_schema.get("description").and_then(|d| d.as_str()) else {
                continue;
            };
            assert!(
                !description.contains("hash-prefixed"),
                "tool {} parameter {param_name:?} description still mentions the obsolete \
                 \"hash-prefixed\" session-id format (removed in the migration squash). \
                 Use \"a canonical UUID\" instead. Description was: {description:?}",
                tool.name,
            );
        }
    }
    Ok(())
}

const CLIENT_DESCRIPTION_LIMIT: usize = 2048;

#[test]
fn every_tool_description_reaches_the_model_whole() {
    for tool in all_tools() {
        let length = tool
            .description
            .as_deref()
            .map(|text| text.chars().count())
            .unwrap_or_default();
        assert!(
            length <= CLIENT_DESCRIPTION_LIMIT,
            "the description of MCP tool {} is {length} characters; Claude Code cuts a tool \
             description at {CLIENT_DESCRIPTION_LIMIT} and the model never sees the rest. \
             Shorten it, or move detail into the parameter descriptions.",
            tool.name,
        );
    }
}
