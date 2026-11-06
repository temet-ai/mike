mod block_walk;
pub(crate) mod content_blocks;
pub(crate) mod matcher;
pub(crate) mod replay;
mod rule_deserialize;
mod rule_validation;
pub mod service;
pub(crate) mod stage;
pub(crate) mod style_check;
mod types;

#[cfg(test)]
mod content_blocks_edge_case_tests;
#[cfg(test)]
mod content_blocks_slice_tests;
#[cfg(test)]
mod content_blocks_splice_tests;
#[cfg(test)]
mod content_blocks_test_support;
#[cfg(test)]
mod content_blocks_tests;
#[cfg(test)]
mod rule_apply_and_codepoint_tests;
#[cfg(test)]
mod rule_deserialize_tests;
#[cfg(test)]
mod rule_scope_tests;
#[cfg(test)]
mod rule_test_support;
#[cfg(test)]
mod rule_validation_tests;

pub(crate) use types::FilterOutcome;
pub use types::{FilterAction, FilterConfig, FilterKind, FilterRule, RuleSource};

pub(crate) fn apply_request_filter(
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

pub(crate) fn normalize_title(title: Option<String>) -> Option<String> {
    title.filter(|t| !t.trim().is_empty())
}

pub(crate) fn canonicalize_project_path(path: &str) -> String {
    let trimmed = path.trim().trim_end_matches('/');
    if !trimmed.starts_with('/') {
        return trimmed.to_string();
    }
    crate::extract::request::compute_project_root(trimmed).to_string()
}

pub(crate) fn project_scope_matches(rule_path: &str, request_project: &str) -> bool {
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
