use super::observed::{ConnectionFacts, McpFacts};
use super::{Check, SetupStep};

pub(super) fn mcp(facts: &McpFacts) -> Vec<Check> {
    let binary = match &facts.binary {
        Some(path) => Check::ok("mcp_binary", "mike-mcp", path.display().to_string()),
        None => Check::warn(
            "mcp_binary",
            "mike-mcp",
            "`mike-mcp` is neither on PATH nor beside `mike`",
            Some(SetupStep::Claude),
        ),
    };
    let registered = match &facts.registration {
        Ok(r) if r.user_scope() && r.command_exists => Check::ok(
            "mcp_registered",
            "mike-mcp in claude",
            format!(
                "registered in ~/.claude.json with {}",
                r.command.as_deref().unwrap_or_default()
            ),
        ),
        Ok(r) if r.user_scope() => Check::warn(
            "mcp_registered",
            "mike-mcp in claude",
            format!(
                "registered in ~/.claude.json with {}, but that file does not exist",
                r.command.as_deref().unwrap_or_default()
            ),
            Some(SetupStep::Claude),
        ),
        Ok(r) if r.in_a_project => Check::ok(
            "mcp_registered",
            "mike-mcp in claude",
            "registered for a project in ~/.claude.json",
        ),
        Ok(_) => Check::warn(
            "mcp_registered",
            "mike-mcp in claude",
            "not registered in ~/.claude.json, so Claude Code cannot call mike's tools",
            Some(SetupStep::Claude),
        ),
        Err(why) => Check::warn(
            "mcp_registered",
            "mike-mcp in claude",
            format!("~/.claude.json could not be read: {why}"),
            Some(SetupStep::Claude),
        ),
    };
    vec![binary, registered]
}

pub(super) fn connection(facts: &ConnectionFacts) -> Vec<Check> {
    let route = match facts.route {
        Some(route) => Check::ok("claude_route", "claude route", route.as_str()),
        None => Check::warn(
            "claude_route",
            "claude route",
            "no route is chosen, so mike does not know whether Claude Code goes through the sidecar or straight to the proxy",
            Some(SetupStep::Claude),
        ),
    };
    let with_block: Vec<&str> = facts
        .shells
        .iter()
        .filter(|s| matches!(s.block, Ok(true)))
        .map(|s| s.shell)
        .collect();
    let function = if with_block.is_empty() {
        Check::warn(
            "shell_function",
            "shell function",
            "no shell startup file has the mike block, so start Claude Code with `mike claude`",
            Some(SetupStep::Claude),
        )
    } else {
        Check::ok(
            "shell_function",
            "shell function",
            format!(
                "`{}` is defined in {}",
                facts.function_name.as_deref().unwrap_or("cc"),
                with_block.join(", ")
            ),
        )
    };
    vec![route, function]
}
