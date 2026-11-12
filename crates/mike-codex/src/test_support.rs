use crate::{ApprovalPolicy, McpBinding, SandboxMode, SeatOptions, ThreadPolicy};

pub(crate) fn options() -> SeatOptions {
    SeatOptions {
        program: "codex".into(),
        cwd: "/repo".into(),
        codex_home: "/state/seats/repo/codex/run-1".into(),
        model: Some("gpt-6.1-sol".into()),
        effort: Some("high".into()),
        developer_instructions: "contract".into(),
        policy: ThreadPolicy {
            sandbox: SandboxMode::WorkspaceWrite,
            approval: ApprovalPolicy::Never,
        },
        mcp: Some(McpBinding {
            name: "mike".into(),
            command: "mike-mcp".into(),
            env: vec![
                ("MIKE_MCP_PROFILE".into(), "worker".into()),
                ("CLAUDE_CODE_SESSION_ID".into(), "s-1".into()),
            ],
        }),
    }
}
