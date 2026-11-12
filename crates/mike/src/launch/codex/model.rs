pub const CODEX_EFFORTS: [&str; 5] = ["minimal", "low", "medium", "high", "xhigh"];

const CLAUDE_MODEL_WORDS: [&str; 5] = ["claude", "opus", "sonnet", "haiku", "fable"];

pub fn model_problem(model: &str) -> Option<String> {
    let name = model.trim();
    if name.is_empty() {
        return None;
    }
    let lower = name.to_ascii_lowercase();
    if let Some(word) = CLAUDE_MODEL_WORDS.iter().find(|w| lower.contains(*w)) {
        return Some(format!(
            "`{name}` names a Claude model ({word}); a Codex seat takes a Codex model id such as gpt-6.1-sol, or leave it empty for the Codex home's default"
        ));
    }
    if name.chars().any(char::is_whitespace) || name.starts_with('-') {
        return Some(format!(
            "`{name}` is not a model id: it holds whitespace or starts with `-`"
        ));
    }
    None
}

pub fn effort_problem(effort: Option<&str>) -> Option<String> {
    let level = effort.map(str::trim).filter(|l| !l.is_empty())?;
    (!CODEX_EFFORTS.contains(&level)).then(|| {
        format!(
            "`{level}` is not a Codex reasoning effort; use {}",
            CODEX_EFFORTS.join(", ")
        )
    })
}

pub fn agent_problem(agent: &str) -> Option<String> {
    let name = agent.trim();
    (!name.is_empty()).then(|| {
        format!(
            "`{name}` is a Claude Code agent (`--agent`), which Codex has no equivalent for; leave agent empty and put the role in the contract"
        )
    })
}
