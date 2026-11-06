use std::collections::HashSet;

use serde::Serialize;
use serde_json::Value;

use super::glob::glob_match;

pub(crate) const DEFAULT_MIN_RESULT_BYTES: usize = 1500;
pub(crate) const DEFAULT_HEAD_LINES: usize = 40;
pub(crate) const DEFAULT_TAIL_LINES: usize = 20;
pub(crate) const DEFAULT_GRACE_TURNS: usize = 3;

pub(super) const POLICY_ID: &str = "file_read";
pub(super) const POLICY_VERSION: u32 = 4;

pub(crate) struct FileReadPolicy<'a> {
    pub(super) capture: Option<crate::compress::recovery::CapturedResults>,
    pub(super) min_result_bytes: usize,
    pub(super) head_lines: usize,
    pub(super) tail_lines: usize,
    pub(super) grace_turns: usize,
    pub(super) enabled_producers: &'a [String],
    protected_paths: &'a [String],
    pub(super) warm_cuts: Option<&'a HashSet<String>>,
}

impl<'a> FileReadPolicy<'a> {
    pub(crate) fn new(
        min_result_bytes: usize,
        (head_lines, tail_lines): (usize, usize),
        grace_turns: usize,
        enabled_producers: &'a [String],
        protected_paths: &'a [String],
    ) -> Self {
        Self {
            capture: None,
            min_result_bytes,
            head_lines,
            tail_lines,
            grace_turns,
            enabled_producers,
            protected_paths,
            warm_cuts: None,
        }
    }

    pub(crate) fn with_warm_cuts(mut self, warm_cuts: Option<&'a HashSet<String>>) -> Self {
        self.warm_cuts = warm_cuts;
        self
    }

    pub(super) fn holds_for_warm_cache(&self, tool_use_id: &str) -> bool {
        self.warm_cuts
            .is_some_and(|earlier| !earlier.contains(tool_use_id))
    }

    pub(super) fn is_enabled_producer(&self, name: &str) -> bool {
        self.enabled_producers.iter().any(|p| p.as_str() == name)
    }

    pub(crate) fn with_capture(mut self, raw: &[u8]) -> Self {
        self.capture = Some(crate::compress::recovery::CapturedResults::new(raw));
        self
    }

    pub(super) fn is_protected(&self, file_path: &str) -> bool {
        self.protected_paths
            .iter()
            .any(|pattern| glob_match(pattern, file_path))
    }
}

#[derive(Debug, Clone, Serialize)]
struct ResolvedPolicyParams {
    min_result_bytes: usize,
    head_lines: usize,
    tail_lines: usize,
    grace_turns: usize,
    enabled_producers: Vec<String>,
    protected_paths: Vec<String>,
}

pub(super) fn policy_value(policy: &FileReadPolicy<'_>) -> Value {
    let snapshot = ResolvedPolicyParams {
        min_result_bytes: policy.min_result_bytes,
        head_lines: policy.head_lines,
        tail_lines: policy.tail_lines,
        grace_turns: policy.grace_turns,
        enabled_producers: policy.enabled_producers.to_vec(),
        protected_paths: policy.protected_paths.to_vec(),
    };
    serde_json::to_value(snapshot).unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use crate::compress::file_reads::tests_support::TestPolicy;

    #[test]
    fn file_read_policy_is_protected_matches_representative_glob_patterns() {
        let tp = TestPolicy::default().with_protected(&[
            "**/CLAUDE.md",
            "**/BRIEF*.md",
            "**/.claude/**",
            "**/docs/plans/**",
            "**/artifacts/threads/**",
            "**/*.rules.md",
        ]);
        let policy = tp.policy();
        assert!(policy.is_protected("/repo/CLAUDE.md"));
        assert!(policy.is_protected("/repo/artifacts/threads/foo/BRIEF.md"));
        assert!(policy.is_protected("/repo/.claude/agents/rust-developer.md"));
        assert!(policy.is_protected("/repo/docs/plans/foo-plan.md"));
        assert!(policy.is_protected("/repo/artifacts/threads/foo-thread/worklog.md"));
        assert!(policy.is_protected("/repo/.claude/rules/testing.rules.md"));
        assert!(!policy.is_protected("/repo/src/main.rs"));
    }

    #[test]
    fn file_read_policy_is_protected_matches_only_the_supplied_patterns() {
        let tp = TestPolicy::default().with_protected(&["**/only-this.md"]);
        let policy = tp.policy();
        assert!(policy.is_protected("/a/b/only-this.md"));
        assert!(!policy.is_protected("/repo/CLAUDE.md"));
    }
}
