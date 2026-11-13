use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::store::models::DiagnosticCategory;

use super::patterns::Enforcement;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AnalysisConfig {
    pub enabled: bool,
    pub categories: HashMap<DiagnosticCategory, CategoryConfig>,
    pub patterns: HashMap<String, PatternConfig>,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            categories: HashMap::new(),
            patterns: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CategoryConfig {
    pub enabled: bool,
    pub false_positive_filters: Vec<String>,
}

impl Default for CategoryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            false_positive_filters: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PatternConfig {
    pub advice: Option<String>,
    pub enforcement: Option<Enforcement>,
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn default_config_has_every_category_enabled_implicitly() {
        let cfg = AnalysisConfig::default();
        assert!(cfg.enabled, "top-level enabled defaults to true");
        assert!(
            cfg.categories.is_empty(),
            "no entries means every category defaults to enabled"
        );
    }

    #[test]
    fn default_config_has_empty_pattern_overrides() {
        let cfg = AnalysisConfig::default();
        assert!(
            cfg.patterns.is_empty(),
            "default config must carry no pattern overrides"
        );
    }

    #[test]
    fn minimal_toml_disables_top_level() -> anyhow::Result<()> {
        let cfg: AnalysisConfig = toml::from_str("enabled = false\n")?;
        assert!(!cfg.enabled);
        Ok(())
    }

    #[test]
    fn category_disable_parses() -> anyhow::Result<()> {
        let cfg: AnalysisConfig = toml::from_str(
            r#"
[categories.overselling]
enabled = false
"#,
        )?;
        let overselling = cfg
            .categories
            .get(&DiagnosticCategory::Overselling)
            .ok_or_else(|| anyhow::anyhow!("overselling entry missing"))?;
        assert!(!overselling.enabled);
        Ok(())
    }

    #[test]
    fn false_positive_filters_parse() -> anyhow::Result<()> {
        let cfg: AnalysisConfig = toml::from_str(
            r#"
[categories.justifying_shortcuts]
false_positive_filters = ["pragmatic approach"]
"#,
        )?;
        let js = cfg
            .categories
            .get(&DiagnosticCategory::JustifyingShortcuts)
            .ok_or_else(|| anyhow::anyhow!("justifying_shortcuts entry missing"))?;
        assert_eq!(js.false_positive_filters, vec!["pragmatic approach"]);
        assert!(js.enabled, "missing enabled field defaults to true");
        Ok(())
    }

    #[test]
    fn pattern_advice_override_parses() -> anyhow::Result<()> {
        let cfg: AnalysisConfig = toml::from_str(
            r#"
[patterns.pragmatically]
advice = "operator-supplied advice text"
"#,
        )?;
        let override_cfg = cfg
            .patterns
            .get("pragmatically")
            .ok_or_else(|| anyhow::anyhow!("pragmatically override missing"))?;
        assert_eq!(
            override_cfg.advice.as_deref(),
            Some("operator-supplied advice text")
        );
        Ok(())
    }

    fn parsed_enforcement(value: &str) -> anyhow::Result<Option<Enforcement>> {
        let cfg: AnalysisConfig =
            toml::from_str(&format!("[patterns.robust]\nenforcement = \"{value}\"\n"))?;
        let override_cfg = cfg
            .patterns
            .get("robust")
            .ok_or_else(|| anyhow::anyhow!("robust override missing"))?;
        assert_eq!(override_cfg.advice, None);
        Ok(override_cfg.enforcement)
    }

    #[test]
    fn pattern_enforcement_override_parses() -> anyhow::Result<()> {
        assert_eq!(parsed_enforcement("strict")?, Some(Enforcement::Strict));
        Ok(())
    }

    #[test]
    fn stored_block_enforcement_still_parses_as_strict() -> anyhow::Result<()> {
        assert_eq!(parsed_enforcement("block")?, Some(Enforcement::Strict));
        Ok(())
    }

    #[test]
    fn strict_enforcement_serialises_as_strict() -> anyhow::Result<()> {
        assert_eq!(
            serde_json::to_value(Enforcement::Strict)?,
            serde_json::json!("strict")
        );
        Ok(())
    }

    #[test]
    fn pattern_advice_override_passes_long_strings_through_verbatim() -> anyhow::Result<()> {
        let long = "a".repeat(2000);
        let body = format!("[patterns.robust]\nadvice = \"{long}\"\n");
        let cfg: AnalysisConfig = toml::from_str(&body)?;
        let advice = cfg
            .patterns
            .get("robust")
            .and_then(|p| p.advice.as_deref())
            .ok_or_else(|| anyhow::anyhow!("robust override missing"))?;
        assert_eq!(advice.chars().count(), 2000);
        assert!(!advice.ends_with('…'), "no ellipsis must be appended");
        Ok(())
    }

    #[test]
    fn config_with_pathological_filter_rejects_at_registry_build() -> anyhow::Result<()> {
        use crate::analysis::AnalyzerRegistry;
        let candidates = [
            r"\w{0,200000}",
            r"[a-zA-Z0-9_ ]{0,200000}",
            r"(?:[a-z]|[A-Z]|[0-9]|_){0,100000}",
        ];
        let mut rejection_msg: Option<String> = None;
        for pat in candidates {
            let body = format!("[categories.overselling]\nfalse_positive_filters = ['{pat}']\n");
            let cfg: AnalysisConfig = toml::from_str(&body)?;
            if let Err(e) = AnalyzerRegistry::with_defaults(&cfg) {
                rejection_msg = Some(e.to_string());
                break;
            }
        }
        let msg = rejection_msg.ok_or_else(|| {
            anyhow::anyhow!("no candidate pattern busted the size limit at registry build")
        })?;
        assert!(
            msg.contains("overselling"),
            "rejection must name the offending category: {msg}"
        );
        Ok(())
    }
}
