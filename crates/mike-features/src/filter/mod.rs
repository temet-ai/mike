pub mod block_walk;
pub mod content_blocks;
pub mod matcher;
pub mod replay;
pub mod rule_deserialize;
pub mod rule_validation;
pub mod service;
pub mod stage;
pub mod style_check;
pub mod types;

#[cfg(test)]
pub mod content_blocks_edge_case_tests;
#[cfg(test)]
pub mod content_blocks_slice_tests;
#[cfg(test)]
pub mod content_blocks_splice_tests;
#[cfg(any(test, feature = "test-seams"))]
pub mod content_blocks_test_support;
#[cfg(test)]
pub mod content_blocks_tests;
#[cfg(test)]
pub mod rule_apply_and_codepoint_tests;
#[cfg(test)]
pub mod rule_deserialize_tests;
#[cfg(test)]
pub mod rule_scope_tests;
#[cfg(any(test, feature = "test-seams"))]
pub mod rule_test_support;
#[cfg(test)]
pub mod rule_validation_tests;

pub use types::FilterOutcome;
pub use types::{FilterAction, FilterConfig, FilterKind, FilterRule, RuleSource};

pub fn apply_request_filter(
    raw: &[u8],
    cfg: &FilterConfig,
    project_path: &str,
) -> FilterOutcome {
    if cfg.rules.is_empty() {
        return FilterOutcome::Unchanged;
    }
    let any_applicable = cfg.rules.iter().any(|r| {
        r.enabled
            && r.project_path
                .as_deref()
                .is_none_or(|p| project_scope_matches(p, project_path))
    });
    if !any_applicable {
        return FilterOutcome::Unchanged;
    }
    content_blocks::transform_body(raw, cfg, project_path)
}

pub fn normalize_title(title: Option<String>) -> Option<String> {
    title.filter(|t| !t.trim().is_empty())
}

pub fn canonicalize_project_path(path: &str) -> String {
    let trimmed = path.trim().trim_end_matches('/');
    if !trimmed.starts_with('/') {
        return trimmed.to_string();
    }
    crate::extract::request::compute_project_root(trimmed).to_string()
}

pub fn project_scope_matches(rule_path: &str, request_project: &str) -> bool {
    if rule_path == request_project {
        return true;
    }
    if request_project.is_empty() {
        return false;
    }
    if rule_path.starts_with('/') && canonicalize_project_path(rule_path) == request_project {
        return true;
    }
    crate::extract::request::legacy_trimmed_alias(request_project)
        .is_some_and(|alias| rule_path == alias)
}
