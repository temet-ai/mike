use super::{SettingApplies, SettingKind, SettingMeta, SettingScope};

const DEFAULT_PROMPT_ASSISTANT_INSTRUCTIONS: &str = "You help the operator edit the Claude \
    Code system prompt that mike captures and rewrites. That prompt is shared across every \
    project mike has captured traffic for — a global filter edit reaches all of them, not \
    just the project the operator is looking at right now. You can read: the captured \
    prompt they are looking at, their filter edits, and the project's CLAUDE.md and rules. \
    Before recommending a global edit, check whether the operator's other projects say \
    something that agrees or conflicts, using list_projects and compare_across_projects; \
    when a conflict is project-specific, propose a project-scoped edit instead of a global \
    one. When asked to change something, answer with a before/after pair. Match on the \
    shortest text that is unique to the paragraph, say which models and Claude Code \
    versions the change will reach, and point out where it contradicts the project's own \
    instructions — or another project's. Never save an edit yourself. The operator applies \
    it.";

pub(super) fn entries() -> Vec<SettingMeta> {
    let mut v = model_entries();
    v.extend(read_scope_entries());
    v.extend(apply_and_capture_entries());
    v.extend(budget_entries());
    v.extend(anthropic_entries());
    v.extend(integration_entries());
    v
}

fn integration_entries() -> Vec<SettingMeta> {
    vec![SettingMeta {
        key: "assistant.provider",
        group: "Integrations",
        kind: SettingKind::Enum,
        scope: SettingScope::Global,
        applies: SettingApplies::NextMessage,
        default: serde_json::Value::String("anthropic".to_owned()),
        label: "Assistant provider",
        description: "Which service answers the prompt assistant: Anthropic or Codex.",
        min_i64: None,
        max_i64: None,
        min_f64: None,
        max_f64: None,
        max_len: None,
        enum_variants: Some(&["anthropic", "codex"]),
    }]
}

fn anthropic_entries() -> Vec<SettingMeta> {
    vec![SettingMeta {
        key: "assistant.anthropic_auth_mode",
        group: "Anthropic",
        kind: SettingKind::Enum,
        scope: SettingScope::Global,
        applies: SettingApplies::NextMessage,
        default: serde_json::Value::String("claude_code_login".to_owned()),
        label: "Auth mode",
        description: "How the assistant signs in to Anthropic: this machine's Claude Code \
            login, or the API key saved on the Integrations page.",
        min_i64: None,
        max_i64: None,
        min_f64: None,
        max_f64: None,
        max_len: None,
        enum_variants: Some(&["claude_code_login", "api_key"]),
    }]
}

fn model_entries() -> Vec<SettingMeta> {
    vec![
        SettingMeta {
            key: "prompt_assistant.model",
            group: "Prompt assistant",
            kind: SettingKind::String,
            scope: SettingScope::Global,
            applies: SettingApplies::NextMessage,
            default: serde_json::Value::String("claude-sonnet-5".to_owned()),
            label: "Model",
            description: "The Claude model the assistant runs on.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: Some(256),
            enum_variants: None,
        },
        SettingMeta {
            key: "prompt_assistant.codex_model",
            group: "Prompt assistant",
            kind: SettingKind::String,
            scope: SettingScope::Global,
            applies: SettingApplies::NextMessage,
            default: serde_json::Value::String("gpt-6-astra".to_owned()),
            label: "Codex model",
            description: "The Codex model the assistant runs on when Codex is selected.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: Some(256),
            enum_variants: None,
        },
        SettingMeta {
            key: "prompt_assistant.effort",
            group: "Prompt assistant",
            kind: SettingKind::Enum,
            scope: SettingScope::Global,
            applies: SettingApplies::NextMessage,
            default: serde_json::Value::String("medium".to_owned()),
            label: "Reasoning effort",
            description: "How much the assistant thinks before it answers.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: Some(&["low", "medium", "high"]),
        },
        SettingMeta {
            key: "prompt_assistant.instructions",
            group: "Prompt assistant",
            kind: SettingKind::Text,
            scope: SettingScope::Global,
            applies: SettingApplies::NextMessage,
            default: serde_json::Value::String(DEFAULT_PROMPT_ASSISTANT_INSTRUCTIONS.to_owned()),
            label: "Instructions",
            description: "What the assistant is told before every message. It cannot be blank.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: Some(16_384),
            enum_variants: None,
        },
    ]
}

fn read_scope_entries() -> Vec<SettingMeta> {
    let mut v = read_scope_entries_prompt_and_edits();
    v.extend(read_scope_entries_catalogue_and_memory());
    v
}

fn read_scope_entries_prompt_and_edits() -> Vec<SettingMeta> {
    vec![
        SettingMeta {
            key: "prompt_assistant.read_prompt",
            group: "Prompt assistant",
            kind: SettingKind::Bool,
            scope: SettingScope::Global,
            applies: SettingApplies::NextMessage,
            default: serde_json::Value::Bool(true),
            label: "Read: captured prompt",
            description: "Lets the assistant read the prompt of the request you are looking \
                at.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
        SettingMeta {
            key: "prompt_assistant.read_edits",
            group: "Prompt assistant",
            kind: SettingKind::Bool,
            scope: SettingScope::Global,
            applies: SettingApplies::NextMessage,
            default: serde_json::Value::Bool(true),
            label: "Read: filter edits",
            description: "Lets the assistant read your filter rules.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
        SettingMeta {
            key: "prompt_assistant.read_project_instructions",
            group: "Prompt assistant",
            kind: SettingKind::Bool,
            scope: SettingScope::Global,
            applies: SettingApplies::NextMessage,
            default: serde_json::Value::Bool(true),
            label: "Read: project instructions",
            description: "Lets the assistant read the project's CLAUDE.md and the files under \
                .claude/rules.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
    ]
}

fn read_scope_entries_catalogue_and_memory() -> Vec<SettingMeta> {
    vec![
        SettingMeta {
            key: "prompt_assistant.read_versions",
            group: "Prompt assistant",
            kind: SettingKind::Bool,
            scope: SettingScope::Global,
            applies: SettingApplies::NextMessage,
            default: serde_json::Value::Bool(true),
            label: "Read: prompt versions",
            description: "Lets the assistant read which models and Claude Code versions a \
                prompt reaches.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
        SettingMeta {
            key: "prompt_assistant.read_memory",
            group: "Prompt assistant",
            kind: SettingKind::Bool,
            scope: SettingScope::Global,
            applies: SettingApplies::NextMessage,
            default: serde_json::Value::Bool(false),
            label: "Read: project memory",
            description: "Lets the assistant read the project's Claude Code memory files under \
                ~/.claude/projects/. Off by default because those files can hold private notes.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
        SettingMeta {
            key: "prompt_assistant.read_other_projects",
            group: "Prompt assistant",
            kind: SettingKind::Bool,
            scope: SettingScope::Global,
            applies: SettingApplies::NextMessage,
            default: serde_json::Value::Bool(true),
            label: "Read: other projects",
            description: "Lets the assistant check your other projects for a conflict before \
                it suggests an edit to the prompt they share.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
    ]
}

fn apply_and_capture_entries() -> Vec<SettingMeta> {
    vec![
        SettingMeta {
            key: "prompt_assistant.may_apply",
            group: "Prompt assistant",
            kind: SettingKind::Enum,
            scope: SettingScope::Global,
            applies: SettingApplies::NextMessage,
            default: serde_json::Value::String("suggest".to_owned()),
            label: "May apply edits",
            description: "Suggest: the assistant proposes an edit and you apply it. Apply: the \
                assistant applies it when you ask.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: Some(&["suggest", "apply"]),
        },
        SettingMeta {
            key: "prompt_assistant.capture",
            group: "Prompt assistant",
            kind: SettingKind::Bool,
            scope: SettingScope::Global,
            applies: SettingApplies::NextMessage,
            default: serde_json::Value::Bool(true),
            label: "Capture assistant traffic",
            description: "Sends the assistant's own traffic through mike, so you can inspect \
                it like any other session.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
        SettingMeta {
            key: "prompt_assistant.max_turns",
            group: "Prompt assistant",
            kind: SettingKind::Int,
            scope: SettingScope::Global,
            applies: SettingApplies::NextMessage,
            default: serde_json::Value::Number(serde_json::Number::from(8u64)),
            label: "Max turns per message",
            description: "How many steps the assistant may take before it must answer.",
            min_i64: Some(1),
            max_i64: Some(40),
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
    ]
}

fn budget_entries() -> Vec<SettingMeta> {
    vec![
        SettingMeta {
            key: "prompt_assistant.daily_budget_usd",
            group: "Prompt assistant",
            kind: SettingKind::Float,
            scope: SettingScope::Global,
            applies: SettingApplies::NextMessage,
            default: serde_json::Value::from(1.0_f64),
            label: "Daily budget (USD)",
            description: "The most the assistant may spend in one day. Past it, messages are \
                refused until the next day.",
            min_i64: None,
            max_i64: None,
            min_f64: Some(0.0),
            max_f64: Some(100.0),
            max_len: None,
            enum_variants: None,
        },
        SettingMeta {
            key: "prompt_assistant.keep_days",
            group: "Prompt assistant",
            kind: SettingKind::Int,
            scope: SettingScope::Global,
            applies: SettingApplies::NextCycle,
            default: serde_json::Value::Number(serde_json::Number::from(30i64)),
            label: "Keep conversations (days)",
            description: "How many days a conversation is kept before it is deleted.",
            min_i64: Some(1),
            max_i64: Some(365),
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
    ]
}

#[cfg(test)]
#[path = "meta_assistants_tests.rs"]
mod tests;
