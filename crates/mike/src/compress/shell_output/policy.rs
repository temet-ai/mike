use std::collections::HashSet;

use serde::Serialize;
use serde_json::Value;

use crate::compress::recovery::CapturedResults;

use super::reduction::ReductionShape;

pub(crate) use crate::settings::meta_shell_output::{
    DEFAULT_GRACE_TURNS, DEFAULT_HEAD_LINES, DEFAULT_MIN_RESULT_BYTES,
    DEFAULT_SIGNAL_CONTEXT_LINES, DEFAULT_SIGNAL_LINE_CAP, DEFAULT_TAIL_LINES,
};
pub(crate) const DEFAULT_MIN_SHRINK_PERCENT: usize = 25;
pub(crate) const PRODUCER: &str = "Bash";

pub(super) const POLICY_ID: &str = "shell_output";
pub(crate) const POLICY_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub(crate) struct ShellOutputLimits {
    pub(crate) min_result_bytes: usize,
    pub(crate) head_lines: usize,
    pub(crate) tail_lines: usize,
    pub(crate) signal_context_lines: usize,
    pub(crate) signal_line_cap: usize,
    pub(crate) grace_turns: usize,
    pub(crate) min_shrink_percent: usize,
}

impl Default for ShellOutputLimits {
    fn default() -> Self {
        Self {
            min_result_bytes: DEFAULT_MIN_RESULT_BYTES,
            head_lines: DEFAULT_HEAD_LINES,
            tail_lines: DEFAULT_TAIL_LINES,
            signal_context_lines: DEFAULT_SIGNAL_CONTEXT_LINES,
            signal_line_cap: DEFAULT_SIGNAL_LINE_CAP,
            grace_turns: DEFAULT_GRACE_TURNS,
            min_shrink_percent: DEFAULT_MIN_SHRINK_PERCENT,
        }
    }
}

pub(crate) struct ShellOutputPolicy<'a> {
    pub(super) limits: ShellOutputLimits,
    pub(super) capture: Option<CapturedResults>,
    pub(super) warm_cuts: Option<&'a HashSet<String>>,
}

impl<'a> ShellOutputPolicy<'a> {
    pub(crate) fn new(limits: ShellOutputLimits) -> Self {
        Self {
            limits,
            capture: None,
            warm_cuts: None,
        }
    }

    pub(crate) fn with_capture(mut self, raw: &[u8]) -> Self {
        self.capture = Some(CapturedResults::new(raw));
        self
    }

    pub(crate) fn with_warm_cuts(mut self, warm_cuts: Option<&'a HashSet<String>>) -> Self {
        self.warm_cuts = warm_cuts;
        self
    }

    pub(super) fn holds_for_warm_cache(&self, tool_use_id: &str) -> bool {
        self.warm_cuts
            .is_some_and(|earlier| !earlier.contains(tool_use_id))
    }

    pub(super) fn shape(&self) -> ReductionShape {
        ReductionShape {
            head_lines: self.limits.head_lines,
            tail_lines: self.limits.tail_lines,
            signal_context_lines: self.limits.signal_context_lines,
            signal_line_cap: self.limits.signal_line_cap,
        }
    }

    pub(super) fn shrinks_enough(&self, original: usize, reduced: usize) -> bool {
        reduced * 100 <= original * (100 - self.limits.min_shrink_percent.min(100))
    }
}

pub(super) fn policy_value(policy: &ShellOutputPolicy<'_>) -> Value {
    serde_json::to_value(policy.limits).unwrap_or(Value::Null)
}
