use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeatKind {
    Coordinator,
    Worker,
    Observer,
    Acceptance,
    Reference,
}

impl SeatKind {
    pub fn from_profile(profile: &str) -> Self {
        match profile {
            "coordinator" => Self::Coordinator,
            "observer" => Self::Observer,
            "acceptance" => Self::Acceptance,
            "reference" => Self::Reference,
            _ => Self::Worker,
        }
    }

    pub fn profile(self) -> &'static str {
        match self {
            Self::Coordinator => "coordinator",
            Self::Worker => "worker",
            Self::Observer => "observer",
            Self::Acceptance => "acceptance",
            Self::Reference => "reference",
        }
    }
}

pub fn render_contract(kind: SeatKind, seat: &str, session: Uuid, contract: &str) -> String {
    let profile = kind.profile();
    let mut text = format!(
        "You are seat {seat}, the {profile} of a mike fleet, running in Codex.\n\
         Your mike session id is {session}. The `mike` MCP server is bound to this seat: \
         it already knows who you are, and wherever a tool takes `session_id` or `from` \
         you pass {session}.\n\
         The contract below was written for every runtime. Read it with these \
         substitutions for Codex:\n\
         - A tool written `mcp__mike__<name>` is the `<name>` tool of the `mike` MCP server.\n\
         - Claude Code's Agent, Skill, ToolSearch and /goal do not exist here; do that work \
         yourself in this thread.\n\
         - Mail from other seats arrives as a new user message that starts with `Mail from`.\n\
         - Commit only through the mike `fleet_commit` tool; never run git add, commit or push.\n"
    );
    let body = contract.trim();
    if !body.is_empty() {
        text.push_str("\n--- contract ---\n");
        text.push_str(body);
        text.push('\n');
    }
    text
}
