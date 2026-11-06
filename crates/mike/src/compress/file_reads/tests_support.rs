use super::*;

pub(super) fn raw_in_history(mut body: Value) -> anyhow::Result<Vec<u8>> {
    if let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) {
        messages.push(serde_json::json!(
            {"role": "assistant", "content": [{"type": "text", "text": "noted"}]}
        ));
        messages.push(serde_json::json!(
            {"role": "user", "content": [{"type": "text", "text": "continue"}]}
        ));
    }
    Ok(serde_json::to_vec(&body)?)
}

pub(super) fn numbered_lines(first: u64, count: u64) -> String {
    (first..first + count)
        .map(|n| format!("{n:>6}\tline {n} of the file with enough padding to matter"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn big_file_content(lines: usize) -> String {
    (1..=lines)
        .map(|i| format!("line {i} of the file with enough padding to matter\n"))
        .collect()
}

pub(super) struct TestPolicy {
    pub(super) min_result_bytes: usize,
    pub(super) head_lines: usize,
    pub(super) tail_lines: usize,
    pub(super) grace_turns: usize,
    pub(super) enabled_producers: Vec<String>,
    pub(super) protected_paths: Vec<String>,
}

impl Default for TestPolicy {
    fn default() -> Self {
        Self {
            min_result_bytes: DEFAULT_MIN_RESULT_BYTES,
            head_lines: DEFAULT_HEAD_LINES,
            tail_lines: DEFAULT_TAIL_LINES,
            grace_turns: 0,
            enabled_producers: vec!["Read".to_string()],
            protected_paths: Vec::new(),
        }
    }
}

impl TestPolicy {
    pub(super) fn with_producers(mut self, names: &[&str]) -> Self {
        self.enabled_producers = names.iter().map(|s| (*s).to_string()).collect();
        self
    }

    pub(super) fn with_protected(mut self, patterns: &[&str]) -> Self {
        self.protected_paths = patterns.iter().map(|s| (*s).to_string()).collect();
        self
    }

    pub(super) fn with_grace(mut self, turns: usize) -> Self {
        self.grace_turns = turns;
        self
    }

    pub(super) fn policy(&self) -> FileReadPolicy<'_> {
        FileReadPolicy::new(
            self.min_result_bytes,
            (self.head_lines, self.tail_lines),
            self.grace_turns,
            &self.enabled_producers,
            &self.protected_paths,
        )
    }
}
