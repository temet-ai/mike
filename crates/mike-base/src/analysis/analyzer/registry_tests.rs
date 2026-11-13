use super::*;
use crate::analysis::categories::{
    AvoidingWorkAnalyzer, DeflectingBlameAnalyzer, FakingUnderstandingAnalyzer,
    JustifyingShortcutsAnalyzer, OversellingAnalyzer,
};

fn single_category_registry() -> AnalysisConfig {
    AnalysisConfig::default()
}

#[test]
fn justifying_shortcuts_dedups_to_one_with_other_hits() -> anyhow::Result<()> {
    let reg = AnalyzerRegistry::with_defaults(&single_category_registry())?;
    let text = "This is a pragmatic approach and a lightweight implementation for now.";
    let diags: Vec<Diagnostic> = reg
        .run(text)
        .into_iter()
        .filter(|d| d.category == DiagnosticCategory::JustifyingShortcuts)
        .collect();
    assert_eq!(
        diags.len(),
        1,
        "category must dedupe to a single diagnostic"
    );
    assert!(
        diags[0].other_hits >= 1,
        "other_hits must count additional patterns in the same category (got {})",
        diags[0].other_hits
    );
    Ok(())
}

#[test]
fn deflecting_blame_dedups_to_one_with_other_hits() -> anyhow::Result<()> {
    let reg = AnalyzerRegistry::with_defaults(&single_category_registry())?;
    let text = "This is a pre-existing issue. It appears the fix seems to be a known issue.";
    let diags: Vec<Diagnostic> = reg
        .run(text)
        .into_iter()
        .filter(|d| d.category == DiagnosticCategory::DeflectingBlame)
        .collect();
    assert_eq!(diags.len(), 1);
    assert!(diags[0].other_hits >= 1);
    Ok(())
}

#[test]
fn avoiding_work_dedups_to_one_with_other_hits() -> anyhow::Result<()> {
    let reg = AnalyzerRegistry::with_defaults(&single_category_registry())?;
    let text = "This is straightforward — out of scope, I'll revisit this later.";
    let diags: Vec<Diagnostic> = reg
        .run(text)
        .into_iter()
        .filter(|d| d.category == DiagnosticCategory::AvoidingWork)
        .collect();
    assert_eq!(diags.len(), 1);
    assert!(diags[0].other_hits >= 1);
    Ok(())
}

#[test]
fn faking_understanding_dedups_to_one_with_other_hits() -> anyhow::Result<()> {
    let reg = AnalyzerRegistry::with_defaults(&single_category_registry())?;
    let text =
        "Consistent with existing pattern following the convention and aligning with design.";
    let diags: Vec<Diagnostic> = reg
        .run(text)
        .into_iter()
        .filter(|d| d.category == DiagnosticCategory::FakingUnderstanding)
        .collect();
    assert_eq!(diags.len(), 1);
    assert!(diags[0].other_hits >= 1);
    Ok(())
}

#[test]
fn overselling_dedups_to_one_with_other_hits() -> anyhow::Result<()> {
    let reg = AnalyzerRegistry::with_defaults(&single_category_registry())?;
    let text = "A robust and comprehensive production-ready clean implementation.";
    let diags: Vec<Diagnostic> = reg
        .run(text)
        .into_iter()
        .filter(|d| d.category == DiagnosticCategory::Overselling)
        .collect();
    assert_eq!(diags.len(), 1);
    assert!(diags[0].other_hits >= 1);
    Ok(())
}

#[test]
fn registry_run_returns_one_diagnostic_per_triggered_category() -> anyhow::Result<()> {
    let reg = AnalyzerRegistry::with_defaults(&AnalysisConfig::default())?;
    let text = "This pragmatic and straightforward patch is robust.";
    let diags = reg.run(text);
    assert_eq!(
        diags.len(),
        3,
        "expected exactly 3 diagnostics for 3 triggered categories, got {diags:?}"
    );
    let mut cats: Vec<DiagnosticCategory> = diags.iter().map(|d| d.category.clone()).collect();
    cats.sort_by_key(|c| format!("{c:?}"));
    assert!(cats.contains(&DiagnosticCategory::JustifyingShortcuts));
    assert!(cats.contains(&DiagnosticCategory::AvoidingWork));
    assert!(cats.contains(&DiagnosticCategory::Overselling));
    Ok(())
}

#[test]
fn registry_run_empty_text_returns_nothing() -> anyhow::Result<()> {
    let reg = AnalyzerRegistry::with_defaults(&AnalysisConfig::default())?;
    assert!(reg.run("").is_empty());
    Ok(())
}

#[test]
fn registry_run_disabled_top_level_returns_nothing() -> anyhow::Result<()> {
    let cfg = AnalysisConfig {
        enabled: false,
        ..AnalysisConfig::default()
    };
    let reg = AnalyzerRegistry::with_defaults(&cfg)?;
    assert!(
        reg.run("This pragmatic and robust production-ready code.")
            .is_empty()
    );
    Ok(())
}

#[test]
fn registry_wires_all_five_analyzers() -> anyhow::Result<()> {
    let reg = AnalyzerRegistry::with_defaults(&AnalysisConfig::default())?;
    let categories: Vec<DiagnosticCategory> =
        reg.entries.iter().map(|e| e.analyzer.category()).collect();
    assert!(categories.contains(&DiagnosticCategory::JustifyingShortcuts));
    assert!(categories.contains(&DiagnosticCategory::DeflectingBlame));
    assert!(categories.contains(&DiagnosticCategory::AvoidingWork));
    assert!(categories.contains(&DiagnosticCategory::FakingUnderstanding));
    assert!(categories.contains(&DiagnosticCategory::Overselling));
    Ok(())
}

#[test]
fn snapshot_reports_five_analyzers_enabled_by_default() -> anyhow::Result<()> {
    let reg = AnalyzerRegistry::with_defaults(&AnalysisConfig::default())?;
    let snap = reg.snapshot();
    assert_eq!(
        snap.len(),
        5,
        "expected one snapshot row per wired analyzer"
    );
    assert!(
        snap.iter().all(|e| e.enabled),
        "every analyzer must be enabled by default (no-default-off)"
    );
    assert!(
        snap.iter()
            .any(|e| e.name == "overselling_analyzer"
                && e.category == DiagnosticCategory::Overselling),
        "expected an overselling_analyzer row: {:?}",
        snap.iter().map(|e| e.name).collect::<Vec<_>>()
    );
    Ok(())
}

#[test]
fn snapshot_reflects_per_category_disabled_state() -> anyhow::Result<()> {
    let mut cfg = AnalysisConfig::default();
    cfg.categories.insert(
        DiagnosticCategory::Overselling,
        CategoryConfig {
            enabled: false,
            false_positive_filters: vec![],
        },
    );
    let reg = AnalyzerRegistry::with_defaults(&cfg)?;
    let snap = reg.snapshot();
    let overselling = snap
        .iter()
        .find(|e| e.category == DiagnosticCategory::Overselling)
        .ok_or_else(|| anyhow::anyhow!("missing overselling row"))?;
    assert!(
        !overselling.enabled,
        "disabled category must report enabled: false"
    );
    assert!(
        snap.iter()
            .filter(|e| e.category != DiagnosticCategory::Overselling)
            .all(|e| e.enabled)
    );
    Ok(())
}

#[test]
fn snapshot_reflects_top_level_disabled_state() -> anyhow::Result<()> {
    let cfg = AnalysisConfig {
        enabled: false,
        ..AnalysisConfig::default()
    };
    let reg = AnalyzerRegistry::with_defaults(&cfg)?;
    let snap = reg.snapshot();
    assert!(
        snap.iter().all(|e| !e.enabled),
        "top-level disabled must zero out every analyzer's enabled flag"
    );
    Ok(())
}

#[test]
fn snapshot_with_overrides_empty_map_matches_snapshot() -> anyhow::Result<()> {
    let reg = AnalyzerRegistry::with_defaults(&AnalysisConfig::default())?;
    let overrides = HashMap::new();
    let snap = reg.snapshot();
    let overridden = reg.snapshot_with_overrides(&overrides);
    assert_eq!(snap.len(), overridden.len());
    for (a, b) in snap.iter().zip(overridden.iter()) {
        assert_eq!(a.name, b.name);
        assert_eq!(a.enabled, b.enabled);
    }
    Ok(())
}

#[test]
fn snapshot_with_overrides_flips_only_the_named_analyzer() -> anyhow::Result<()> {
    let reg = AnalyzerRegistry::with_defaults(&AnalysisConfig::default())?;
    let mut overrides = HashMap::new();
    overrides.insert("overselling_analyzer".to_string(), false);

    let snap = reg.snapshot_with_overrides(&overrides);
    let overselling = snap
        .iter()
        .find(|e| e.name == "overselling_analyzer")
        .ok_or_else(|| anyhow::anyhow!("missing overselling row"))?;
    assert!(!overselling.enabled, "override must disable this row");
    assert!(
        snap.iter()
            .filter(|e| e.name != "overselling_analyzer")
            .all(|e| e.enabled),
        "un-overridden rows must keep global (enabled) state"
    );
    Ok(())
}

#[test]
fn snapshot_with_overrides_can_enable_a_globally_disabled_analyzer() -> anyhow::Result<()> {
    let mut cfg = AnalysisConfig::default();
    cfg.categories.insert(
        DiagnosticCategory::Overselling,
        CategoryConfig {
            enabled: false,
            false_positive_filters: vec![],
        },
    );
    let reg = AnalyzerRegistry::with_defaults(&cfg)?;
    let mut overrides = HashMap::new();
    overrides.insert("overselling_analyzer".to_string(), true);

    let snap = reg.snapshot_with_overrides(&overrides);
    let overselling = snap
        .iter()
        .find(|e| e.name == "overselling_analyzer")
        .ok_or_else(|| anyhow::anyhow!("missing overselling row"))?;
    assert!(
        overselling.enabled,
        "a per-session override must win over global disabled state"
    );
    Ok(())
}

#[test]
fn analyzer_names_are_stable() {
    assert_eq!(
        JustifyingShortcutsAnalyzer.name(),
        "justifying_shortcuts_analyzer"
    );
    assert_eq!(DeflectingBlameAnalyzer.name(), "deflecting_blame_analyzer");
    assert_eq!(AvoidingWorkAnalyzer.name(), "avoiding_work_analyzer");
    assert_eq!(
        FakingUnderstandingAnalyzer.name(),
        "faking_understanding_analyzer"
    );
    assert_eq!(OversellingAnalyzer.name(), "overselling_analyzer");
}
