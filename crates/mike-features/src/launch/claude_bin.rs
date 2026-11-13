pub fn resolve_claude_binary(env_override: Option<String>) -> String {
    env_override
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "claude".to_string())
}

pub fn claude_binary() -> String {
    resolve_claude_binary(std::env::var("MIKE_CLAUDE_BIN").ok())
}
