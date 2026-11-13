use dashmap::DashSet;

pub type SessionsWithMikeTools = DashSet<String>;

pub fn session_brought_its_own(
    known: &SessionsWithMikeTools,
    session_id: &str,
    body: &[u8],
) -> bool {
    if known.contains(session_id) {
        return true;
    }
    if !names_every_mcp_tool(body) {
        return false;
    }
    known.insert(session_id.to_owned());
    true
}

fn names_every_mcp_tool(body: &[u8]) -> bool {
    super::catalogue()
        .iter()
        .all(|tool| names_mcp_tool(body, tool.mcp_tool))
}

fn names_mcp_tool(body: &[u8], mcp_tool: &str) -> bool {
    let suffix = format!("__{mcp_tool}");
    let suffix = suffix.as_bytes();
    body.windows(suffix.len())
        .enumerate()
        .any(|(at, window)| window == suffix && follows_an_mcp_server_name(&body[..at]))
}

fn follows_an_mcp_server_name(before: &[u8]) -> bool {
    const PREFIX: &[u8] = b"mcp__";
    let server_len = before
        .iter()
        .rev()
        .take_while(|b| b.is_ascii_alphanumeric() || **b == b'-' || **b == b'_')
        .count();
    let name = &before[before.len() - server_len..];
    name.windows(PREFIX.len())
        .position(|window| window == PREFIX)
        .is_some_and(|at| at + PREFIX.len() < name.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOADED: &[u8] =
        br#"{"messages":[],"tools":[{"name":"Read"},{"name":"mcp__mike__get_workspace_roster"}]}"#;
    const DEFERRED: &[u8] = br#"{"messages":[{"role":"user","content":"deferred tools: mcp__mike__get_request\nmcp__mike__get_workspace_roster\nmcp__storybook__preview-stories"}],"tools":[{"name":"ToolSearch"}]}"#;
    const WITHOUT: &[u8] =
        br#"{"messages":[{"role":"user","content":"hi"}],"tools":[{"name":"Read"}]}"#;

    #[test]
    fn a_session_whose_tool_list_carries_the_mcp_tool_brought_its_own() {
        let known = SessionsWithMikeTools::new();
        assert!(session_brought_its_own(&known, "s1", LOADED));
    }

    #[test]
    fn a_session_that_lists_the_mcp_tool_as_loadable_brought_its_own() {
        let known = SessionsWithMikeTools::new();
        assert!(session_brought_its_own(&known, "s1", DEFERRED));
    }

    #[test]
    fn a_session_without_the_mcp_server_did_not() {
        let known = SessionsWithMikeTools::new();
        assert!(!session_brought_its_own(&known, "s1", WITHOUT));
        assert!(known.is_empty());
    }

    #[test]
    fn once_seen_the_session_is_remembered_for_bodies_that_no_longer_name_the_tool() {
        let known = SessionsWithMikeTools::new();
        assert!(session_brought_its_own(&known, "s1", DEFERRED));
        assert!(session_brought_its_own(&known, "s1", WITHOUT));
    }

    #[test]
    fn one_session_bringing_its_own_says_nothing_about_another() {
        let known = SessionsWithMikeTools::new();
        assert!(session_brought_its_own(&known, "s1", LOADED));
        assert!(!session_brought_its_own(&known, "s2", WITHOUT));
    }

    #[test]
    fn any_server_name_counts_not_only_mike() {
        let known = SessionsWithMikeTools::new();
        let body = br#"{"tools":[{"name":"mcp__mike-local__get_workspace_roster"}]}"#;
        assert!(session_brought_its_own(&known, "s1", body));
    }

    #[test]
    fn the_bare_tool_name_in_prose_is_not_a_tool_the_session_has() {
        let known = SessionsWithMikeTools::new();
        let body = br#"{"messages":[{"role":"user","content":"call get_workspace_roster or my__get_workspace_roster"}],"tools":[]}"#;
        assert!(!session_brought_its_own(&known, "s1", body));
    }
}
