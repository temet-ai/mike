pub const CHANNEL_OPEN: &str = "<channel ";
pub const TASK_NOTIFICATION: &str = "<task-notification>";
pub const TEAMMATE_MESSAGE: &str = "<teammate-message";
pub const AGENT_MESSAGE: &str = "<agent-message from=";
pub const STOP_HOOK_FEEDBACK: &str = "Stop hook feedback:";
pub const SYSTEM_NOTIFICATION: &str = "[SYSTEM NOTIFICATION - NOT USER INPUT]";
pub const TOOL_LOADED: &str = "Tool loaded.";
pub const AUTONOMOUS_LOOP_DYNAMIC: &str = "<<autonomous-loop-dynamic>>";
pub const AUTONOMOUS_LOOP: &str = "<<autonomous-loop>>";
pub const FLEET_CYCLE_WAKE: &str = "Fleet coordinator cycle wake";
pub const CROSS_SESSION_MESSAGE: &str = "Cross-session message from";
pub const PLUGIN_PROMPT_NOTICE: &str =
    "This is how Claude Code surfaces a prompt a plugin submits between turns";
pub const STEPPED_AWAY: &str = "The user stepped away and is coming back";
pub const STEERING_NOTE: &str = "Steering note from the operator's proxy:";
pub const COMPACTION_TEXT_ONLY: &str = "Respond with TEXT ONLY. Do NOT call any tools.";
pub const COMPACTION_SUMMARY: &str = "detailed summary of the conversation";
pub const MID_TURN_LEAD_IN: &str = "The user sent a new message while you were working:";
pub const MID_TURN_NOTICE: &str =
    "This is how Claude Code surfaces messages the user sends mid-turn";
pub const MID_TURN_NOTICE_END: &str = "Address the message above as you continue this turn";

pub const PROXY_TOOL_RESULT: &str = "proxy tool result for '";
pub const WEASEL_CORRECTION: &str = "weasel correction for '";

pub const WORKFLOW_SUBAGENT_MARKERS: [&str; 2] = [
    "running inside a workflow script",
    "spawned by a workflow orchestration script",
];

pub const WORKFLOW_HARNESS_USER_PREFIX: &str = "[Workflow harness —";
