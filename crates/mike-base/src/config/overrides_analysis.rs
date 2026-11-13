use crate::store::models::DiagnosticCategory;

use super::schema::ProxyConfig;

pub fn apply_analysis_override(
    cfg: &mut ProxyConfig,
    key: &str,
    value: &serde_json::Value,
) -> bool {
    match key {
        "analysis.enabled" => {
            if let Some(v) = value.as_bool() {
                cfg.analysis.enabled = v;
            }
        }
        "analysis.categories.overselling.enabled" => {
            apply_category_enabled(cfg, DiagnosticCategory::Overselling, value);
        }
        "analysis.categories.overselling.false_positive_filters" => {
            apply_category_filters(cfg, DiagnosticCategory::Overselling, value);
        }
        "analysis.categories.justifying_shortcuts.enabled" => {
            apply_category_enabled(cfg, DiagnosticCategory::JustifyingShortcuts, value);
        }
        "analysis.categories.justifying_shortcuts.false_positive_filters" => {
            apply_category_filters(cfg, DiagnosticCategory::JustifyingShortcuts, value);
        }
        "analysis.categories.deflecting_blame.enabled" => {
            apply_category_enabled(cfg, DiagnosticCategory::DeflectingBlame, value);
        }
        "analysis.categories.deflecting_blame.false_positive_filters" => {
            apply_category_filters(cfg, DiagnosticCategory::DeflectingBlame, value);
        }
        "analysis.categories.avoiding_work.enabled" => {
            apply_category_enabled(cfg, DiagnosticCategory::AvoidingWork, value);
        }
        "analysis.categories.avoiding_work.false_positive_filters" => {
            apply_category_filters(cfg, DiagnosticCategory::AvoidingWork, value);
        }
        "analysis.categories.faking_understanding.enabled" => {
            apply_category_enabled(cfg, DiagnosticCategory::FakingUnderstanding, value);
        }
        "analysis.categories.faking_understanding.false_positive_filters" => {
            apply_category_filters(cfg, DiagnosticCategory::FakingUnderstanding, value);
        }
        _ => return false,
    }
    true
}

fn apply_category_enabled(
    cfg: &mut ProxyConfig,
    category: DiagnosticCategory,
    value: &serde_json::Value,
) {
    if let Some(v) = value.as_bool() {
        cfg.analysis.categories.entry(category).or_default().enabled = v;
    }
}

fn apply_category_filters(
    cfg: &mut ProxyConfig,
    category: DiagnosticCategory,
    value: &serde_json::Value,
) {
    if let Some(arr) = value.as_array() {
        cfg.analysis
            .categories
            .entry(category)
            .or_default()
            .false_positive_filters = arr
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect();
    }
}
