use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct HookPayload {
    pub hook_event_name: String,
    pub session_id: String,
    #[serde(default)]
    pub turn_id: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub agent_type: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub tool_use_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ThreadScope {
    pub root_thread_id: String,
    pub thread_id: String,
    pub parent_thread_id: Option<String>,
}

impl HookPayload {
    pub fn parse(raw: &[u8]) -> Result<Self, String> {
        let payload: Self = serde_json::from_slice(raw).map_err(|error| {
            format!(
                "hook body is not a Codex hook payload: {error} (line {}, column {})",
                error.line(),
                error.column()
            )
        })?;
        if payload.session_id.trim().is_empty()
            || payload.session_id.contains('\0')
            || payload.session_id.len() > 512
        {
            return Err(format!(
                "hook body session_id {:?} must be nonempty and contain no NUL",
                payload.session_id
            ));
        }
        Ok(payload)
    }

    pub fn scope(&self) -> ThreadScope {
        match self.agent_id.as_deref().filter(|id| !id.trim().is_empty()) {
            Some(child) => ThreadScope {
                root_thread_id: self.session_id.clone(),
                thread_id: child.to_owned(),
                parent_thread_id: Some(self.session_id.clone()),
            },
            None => ThreadScope {
                root_thread_id: self.session_id.clone(),
                thread_id: self.session_id.clone(),
                parent_thread_id: None,
            },
        }
    }

    pub fn event(&self) -> HookEvent {
        HookEvent::from_name(&self.hook_event_name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEvent {
    SessionStart,
    SessionEnd,
    UserPromptSubmit,
    PreModelRequest,
    PostModelRequest,
    PreToolUse,
    PostToolUse,
    PermissionRequest,
    SubagentStart,
    SubagentStop,
    Stop,
    PreCompact,
    PostCompact,
    Interrupt,
    Unknown,
}

impl HookEvent {
    pub(crate) const SUPPORTED: [(&'static str, HookEvent); 14] = [
        ("SessionStart", Self::SessionStart),
        ("SessionEnd", Self::SessionEnd),
        ("UserPromptSubmit", Self::UserPromptSubmit),
        ("PreModelRequest", Self::PreModelRequest),
        ("PostModelRequest", Self::PostModelRequest),
        ("PreToolUse", Self::PreToolUse),
        ("PostToolUse", Self::PostToolUse),
        ("PermissionRequest", Self::PermissionRequest),
        ("SubagentStart", Self::SubagentStart),
        ("SubagentStop", Self::SubagentStop),
        ("Stop", Self::Stop),
        ("PreCompact", Self::PreCompact),
        ("PostCompact", Self::PostCompact),
        ("Interrupt", Self::Interrupt),
    ];

    pub(crate) fn from_name(name: &str) -> Self {
        Self::SUPPORTED
            .iter()
            .find(|(known, _)| *known == name)
            .map_or(Self::Unknown, |(_, event)| *event)
    }
}
