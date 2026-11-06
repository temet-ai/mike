pub(crate) mod answer;
pub(crate) mod client;
pub(crate) mod declare;
pub(crate) mod roster;
pub(crate) mod scanner;
pub(crate) mod session;
pub(crate) mod stage;

pub(crate) struct ProxyTool {
    pub(crate) name: &'static str,
    pub(crate) description: &'static str,
    pub(crate) input_schema: fn() -> serde_json::Value,
    pub(crate) mcp_tool: &'static str,
}

pub(crate) fn catalogue() -> &'static [ProxyTool] {
    &[ProxyTool {
        name: roster::TOOL_NAME,
        description: roster::DESCRIPTION,
        input_schema: roster::input_schema,
        mcp_tool: roster::MCP_TOOL,
    }]
}

pub(crate) fn is_proxy_tool(name: &str) -> bool {
    catalogue().iter().any(|t| t.name == name)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test fixtures; hand-built JSON and fixture rows are expected to parse"
)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_name_is_mike_prefixed_and_unique() {
        let mut seen = std::collections::HashSet::new();
        for tool in catalogue() {
            assert!(
                tool.name.starts_with("mike_"),
                "{} must carry the mike_ prefix so it cannot collide with a client tool",
                tool.name
            );
            assert!(seen.insert(tool.name), "duplicate tool name: {}", tool.name);
        }
    }

    #[test]
    fn is_proxy_tool_recognises_the_catalogue_and_nothing_else() {
        for tool in catalogue() {
            assert!(is_proxy_tool(tool.name));
        }
        assert!(!is_proxy_tool("Read"));
        assert!(!is_proxy_tool("mike_not_a_real_tool"));
    }

    #[test]
    fn every_input_schema_is_an_object_schema() {
        for tool in catalogue() {
            let schema = (tool.input_schema)();
            assert_eq!(
                schema.get("type").and_then(serde_json::Value::as_str),
                Some("object"),
                "{}'s input_schema must be an object schema",
                tool.name
            );
            assert!(
                schema.get("properties").is_some(),
                "{}'s input_schema must carry a properties map",
                tool.name
            );
        }
    }

    #[test]
    fn the_declared_set_serialises_identically_on_every_call() {
        let first = declare::declared_tool_values();
        let second = declare::declared_tool_values();
        assert_eq!(
            serde_json::to_vec(&first).expect("serialise"),
            serde_json::to_vec(&second).expect("serialise"),
            "a declared set that varies between requests turns one cache miss into a per-request tax"
        );
    }
}
