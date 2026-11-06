use super::*;

fn config_with_fp_filter(category: DiagnosticCategory, pattern: &str) -> AnalysisConfig {
    use super::super::config::CategoryConfig;
    let mut cfg = AnalysisConfig::default();
    cfg.categories.insert(
        category,
        CategoryConfig {
            enabled: true,
            false_positive_filters: vec![pattern.to_string()],
        },
    );
    cfg
}

#[test]
fn fp_filter_builder_happy_path_compiles() -> anyhow::Result<()> {
    let cfg = config_with_fp_filter(
        DiagnosticCategory::JustifyingShortcuts,
        "pragmatic approach",
    );
    let reg = AnalyzerRegistry::with_defaults(&cfg)?;
    let diags = reg.run("Using a pragmatic approach to the refactor.");
    let js_present = diags
        .iter()
        .any(|d| d.category == DiagnosticCategory::JustifyingShortcuts);
    assert!(
        !js_present,
        "benign fp filter must still suppress its target sentence"
    );
    Ok(())
}

#[test]
fn fp_filter_size_limit_rejects_pathological_pattern() -> anyhow::Result<()> {
    let candidates = [
        r"\w{0,200000}",
        r"[a-zA-Z0-9_ ]{0,200000}",
        r"(?:[a-z]|[A-Z]|[0-9]|_){0,100000}",
    ];
    let mut rejected = None;
    for pat in candidates {
        let cfg = config_with_fp_filter(DiagnosticCategory::Overselling, pat);
        if let Err(e) = AnalyzerRegistry::with_defaults(&cfg) {
            rejected = Some((pat, e));
            break;
        }
    }
    let (pat, err) = rejected.ok_or_else(|| {
        anyhow::anyhow!("no candidate pattern busted the size limit — candidates may need updating")
    })?;
    let msg = err.to_string();
    assert!(
        msg.contains("overselling"),
        "error must name the offending category: {msg}"
    );
    assert!(
        msg.contains("invalid false-positive filter"),
        "error must name what failed: {msg}"
    );
    assert!(
        msg.contains(pat),
        "error must echo the offending pattern so operators can find it: {msg}"
    );
    Ok(())
}

#[test]
fn fp_filter_invalid_syntax_rejected_with_category_in_error() -> anyhow::Result<()> {
    let cfg = config_with_fp_filter(DiagnosticCategory::AvoidingWork, "(unclosed");
    let err = AnalyzerRegistry::with_defaults(&cfg)
        .err()
        .ok_or_else(|| anyhow::anyhow!("expected syntax rejection but got Ok"))?;
    let msg = err.to_string();
    assert!(
        msg.contains("avoiding_work"),
        "error must name the offending category: {msg}"
    );
    assert!(
        msg.contains("(unclosed"),
        "error must echo the offending pattern: {msg}"
    );
    Ok(())
}
