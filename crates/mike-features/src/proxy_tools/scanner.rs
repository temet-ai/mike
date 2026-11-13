use super::answer::{AssistantPrelude, ProxyToolCall};

struct PendingCall {
    id: String,
    name: String,
    input_json: String,
}

pub struct ToolCallScanner {
    text: String,
    pending: Option<PendingCall>,
    fired: bool,
}

impl ToolCallScanner {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            pending: None,
            fired: false,
        }
    }

    pub fn reset(&mut self) {
        self.text.clear();
        self.pending = None;
        self.fired = false;
    }

    pub fn push_text(&mut self, chunk: &str) {
        if !self.fired {
            self.text.push_str(chunk);
        }
    }

    pub fn open_tool_use(&mut self, id: &str, name: &str) {
        if self.fired || !super::is_proxy_tool(name) {
            return;
        }
        self.pending = Some(PendingCall {
            id: id.to_string(),
            name: name.to_string(),
            input_json: String::new(),
        });
    }

    pub fn push_input_json(&mut self, chunk: &str) {
        if let Some(pending) = self.pending.as_mut() {
            pending.input_json.push_str(chunk);
        }
    }

    pub fn close_block(&mut self) -> Option<(ProxyToolCall, AssistantPrelude)> {
        let pending = self.pending.take()?;
        self.fired = true;
        Some((
            ProxyToolCall {
                id: pending.id,
                name: pending.name,
                input_json: pending.input_json,
            },
            AssistantPrelude {
                text: std::mem::take(&mut self.text),
            },
        ))
    }
}

impl Default for ToolCallScanner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test fixtures; hand-built JSON and fixture rows are expected to parse"
)]
mod tests {
    use super::*;
    use crate::proxy_tools::roster::TOOL_NAME;

    #[test]
    fn reports_a_call_against_a_declared_tool() {
        let mut s = ToolCallScanner::new();
        s.open_tool_use("toolu_1", TOOL_NAME);
        let (call, prelude) = s.close_block().expect("a declared tool must fire");
        assert_eq!(call.id, "toolu_1");
        assert_eq!(call.name, TOOL_NAME);
        assert_eq!(prelude.text, "");
    }

    #[test]
    fn ignores_a_call_against_a_client_tool() {
        let mut s = ToolCallScanner::new();
        s.open_tool_use("toolu_1", "Read");
        s.push_input_json(r#"{"file_path":"/x"}"#);
        assert!(s.close_block().is_none());
    }

    #[test]
    fn accumulates_input_json_across_deltas() {
        let mut s = ToolCallScanner::new();
        s.open_tool_use("toolu_1", TOOL_NAME);
        s.push_input_json(r#"{"scope"#);
        s.push_input_json(r#"":"live"}"#);
        let (call, _) = s.close_block().expect("fires");
        assert_eq!(call.input_json, r#"{"scope":"live"}"#);
    }

    #[test]
    fn carries_assistant_text_emitted_before_the_call() {
        let mut s = ToolCallScanner::new();
        s.push_text("Let me check ");
        s.push_text("who else is here.");
        s.open_tool_use("toolu_1", TOOL_NAME);
        let (_, prelude) = s.close_block().expect("fires");
        assert_eq!(prelude.text, "Let me check who else is here.");
    }

    #[test]
    fn reset_clears_text_and_any_open_block() {
        let mut s = ToolCallScanner::new();
        s.push_text("stale");
        s.open_tool_use("toolu_1", TOOL_NAME);
        s.reset();
        assert!(s.close_block().is_none());
        s.open_tool_use("toolu_2", TOOL_NAME);
        let (call, prelude) = s.close_block().expect("fires");
        assert_eq!(call.id, "toolu_2");
        assert_eq!(prelude.text, "", "prior response's text must not leak");
    }

    #[test]
    fn reports_at_most_one_call_per_response() {
        let mut s = ToolCallScanner::new();
        s.open_tool_use("toolu_1", TOOL_NAME);
        assert!(s.close_block().is_some());
        s.open_tool_use("toolu_2", TOOL_NAME);
        assert!(s.close_block().is_none());
    }

    #[test]
    fn a_block_stop_with_nothing_open_reports_nothing() {
        let mut s = ToolCallScanner::new();
        s.push_text("just prose");
        assert!(s.close_block().is_none());
    }
}
