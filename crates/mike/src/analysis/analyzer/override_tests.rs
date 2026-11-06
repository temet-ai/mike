use super::*;

fn config_with_pattern_override(name: &str, advice: Option<&str>) -> AnalysisConfig {
    use super::super::config::PatternConfig;
    let mut cfg = AnalysisConfig::default();
    cfg.patterns.insert(
        name.to_string(),
        PatternConfig {
            advice: advice.map(String::from),
            enforcement: None,
        },
    );
    cfg
}

fn config_with_enforcement_override(name: &str, enforcement: Enforcement) -> AnalysisConfig {
    use super::super::config::PatternConfig;
    let mut cfg = AnalysisConfig::default();
    cfg.patterns.insert(
        name.to_string(),
        PatternConfig {
            advice: None,
            enforcement: Some(enforcement),
        },
    );
    cfg
}

#[test]
fn advice_override_replaces_builtin_text_for_matching_pattern() -> anyhow::Result<()> {
    let cfg = config_with_pattern_override("robust", Some("custom overselling advice"));
    let reg = AnalyzerRegistry::with_defaults(&cfg)?;
    let diags = reg.run("This is robust.");
    let over = diags
        .iter()
        .find(|d| d.category == DiagnosticCategory::Overselling)
        .ok_or_else(|| anyhow::anyhow!("overselling finding should exist"))?;
    assert_eq!(
        over.advice, "custom overselling advice",
        "advice override must replace built-in text"
    );
    Ok(())
}

#[test]
fn advice_override_for_unknown_pattern_is_ignored_silently() -> anyhow::Result<()> {
    let cfg =
        config_with_pattern_override("this_pattern_name_does_not_exist", Some("replacement text"));
    let reg = AnalyzerRegistry::with_defaults(&cfg)?;
    let diags = reg.run("This is robust.");
    let over = diags
        .iter()
        .find(|d| d.category == DiagnosticCategory::Overselling)
        .ok_or_else(|| anyhow::anyhow!("overselling finding should exist"))?;
    assert_eq!(
        over.advice, "enumerate every error case",
        "unknown-pattern override must be silently ignored"
    );
    Ok(())
}

#[test]
fn advice_override_with_none_variant_is_silently_skipped() -> anyhow::Result<()> {
    let cfg = config_with_pattern_override("robust", None);
    let reg = AnalyzerRegistry::with_defaults(&cfg)?;
    let diags = reg.run("This is robust.");
    let over = diags
        .iter()
        .find(|d| d.category == DiagnosticCategory::Overselling)
        .ok_or_else(|| anyhow::anyhow!("overselling finding should exist"))?;
    assert_eq!(over.advice, "enumerate every error case");
    Ok(())
}

#[test]
fn advice_override_empty_string_treated_as_no_override() -> anyhow::Result<()> {
    let cfg = config_with_pattern_override("robust", Some(""));
    let reg = AnalyzerRegistry::with_defaults(&cfg)?;
    let diags = reg.run("This is robust.");
    let over = diags
        .iter()
        .find(|d| d.category == DiagnosticCategory::Overselling)
        .ok_or_else(|| anyhow::anyhow!("overselling finding should exist"))?;
    assert_eq!(over.advice, "enumerate every error case");

    let cfg = config_with_pattern_override("robust", Some("   \n\t "));
    let reg = AnalyzerRegistry::with_defaults(&cfg)?;
    let diags = reg.run("This is robust.");
    let over = diags
        .iter()
        .find(|d| d.category == DiagnosticCategory::Overselling)
        .ok_or_else(|| anyhow::anyhow!("overselling finding should exist"))?;
    assert_eq!(over.advice, "enumerate every error case");
    Ok(())
}

#[test]
fn patterns_by_enforcement_returns_builtin_class_with_no_overrides() -> anyhow::Result<()> {
    let reg = AnalyzerRegistry::with_defaults(&AnalysisConfig::default())?;
    let strict = reg.patterns_by_enforcement(Enforcement::Strict);
    let names: std::collections::BTreeSet<&str> = strict.iter().map(|p| p.name).collect();
    assert_eq!(
        names,
        ["pre_existing", "out_of_scope", "natural_followup"]
            .into_iter()
            .collect()
    );
    Ok(())
}

#[test]
fn patterns_by_enforcement_reflects_override() -> anyhow::Result<()> {
    let cfg = config_with_enforcement_override("robust", Enforcement::Strict);
    let reg = AnalyzerRegistry::with_defaults(&cfg)?;
    let strict = reg.patterns_by_enforcement(Enforcement::Strict);
    assert!(strict.iter().any(|p| p.name == "robust"));
    let advise = reg.patterns_by_enforcement(Enforcement::Advise);
    assert!(!advise.iter().any(|p| p.name == "robust"));
    Ok(())
}

#[test]
fn enforcement_override_for_unknown_pattern_is_ignored_silently() -> anyhow::Result<()> {
    let cfg =
        config_with_enforcement_override("this_pattern_name_does_not_exist", Enforcement::Strict);
    let reg = AnalyzerRegistry::with_defaults(&cfg)?;
    let strict = reg.patterns_by_enforcement(Enforcement::Strict);
    assert_eq!(
        strict.len(),
        3,
        "unknown-pattern override must not add an entry"
    );
    Ok(())
}
