use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use super::analyzer::AnalyzerRegistry;
use super::patterns::{self, Enforcement};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CorpusEntry {
    pub(crate) name: &'static str,
    pub(crate) category: &'static str,
    pub(crate) regex: String,
    pub(crate) advice: Cow<'static, str>,
    pub(crate) enforcement: Enforcement,
}

#[derive(Debug, Serialize)]
struct Corpus {
    patterns: Vec<CorpusEntry>,
}

fn raw_regex_source(pat: &patterns::CompiledPattern) -> String {
    pat.regex
        .as_str()
        .strip_prefix("(?i)")
        .unwrap_or_else(|| pat.regex.as_str())
        .to_string()
}

fn build_with_overrides(
    advice_overrides: &HashMap<String, String>,
    enforcement_overrides: &HashMap<String, Enforcement>,
) -> Corpus {
    let patterns = patterns::CATEGORIZED
        .iter()
        .flat_map(|(category, pats)| {
            pats.iter().map(move |pat| CorpusEntry {
                name: pat.name,
                category: category.as_str(),
                regex: raw_regex_source(pat),
                advice: advice_overrides
                    .get(pat.name)
                    .map(|s| Cow::Owned(s.clone()))
                    .unwrap_or(Cow::Borrowed(pat.advice)),
                enforcement: enforcement_overrides
                    .get(pat.name)
                    .copied()
                    .unwrap_or(pat.enforcement),
            })
        })
        .collect();
    Corpus { patterns }
}

fn build() -> Corpus {
    build_with_overrides(&HashMap::new(), &HashMap::new())
}

pub(crate) fn effective_entries(registry: &AnalyzerRegistry) -> Vec<CorpusEntry> {
    build_with_overrides(
        registry.advice_overrides(),
        registry.enforcement_overrides(),
    )
    .patterns
}

pub(crate) fn render() -> anyhow::Result<String> {
    let mut json = serde_json::to_string_pretty(&build())?;
    json.push('\n');
    Ok(json)
}

pub(crate) fn write_to_path(path: &Path) -> anyhow::Result<()> {
    let json = render()?;
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMITTED_PATH: &str = "tests/fixtures/analysis-corpus.json";

    fn crate_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
    }

    #[test]
    fn build_produces_one_entry_per_pattern() {
        assert_eq!(build().patterns.len(), 19);
    }

    #[test]
    fn regex_source_strips_case_insensitive_prefix() -> anyhow::Result<()> {
        let corpus = build();
        let robust = corpus
            .patterns
            .iter()
            .find(|p| p.name == "robust")
            .ok_or_else(|| anyhow::anyhow!("robust pattern must be present"))?;
        assert_eq!(robust.regex, r"\brobust\b");
        Ok(())
    }

    #[test]
    fn effective_entries_reports_enforcement_override() -> anyhow::Result<()> {
        use super::super::config::{AnalysisConfig, PatternConfig};

        let mut cfg = AnalysisConfig::default();
        cfg.patterns.insert(
            "robust".to_string(),
            PatternConfig {
                advice: None,
                enforcement: Some(Enforcement::Strict),
            },
        );
        let registry = AnalyzerRegistry::with_defaults(&cfg)?;

        let entries = effective_entries(&registry);
        let robust = entries
            .iter()
            .find(|p| p.name == "robust")
            .ok_or_else(|| anyhow::anyhow!("robust pattern must be present"))?;
        assert_eq!(robust.enforcement, Enforcement::Strict);

        let builtin_robust = build()
            .patterns
            .into_iter()
            .find(|p| p.name == "robust")
            .ok_or_else(|| anyhow::anyhow!("robust pattern must be present"))?;
        assert_ne!(builtin_robust.enforcement, Enforcement::Strict);
        Ok(())
    }

    #[test]
    fn write_to_path_creates_parent_directories() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let out = dir.path().join("nested").join("corpus.json");
        write_to_path(&out)?;
        assert!(out.exists());
        Ok(())
    }

    #[test]
    fn generated_corpus_matches_committed_file() -> anyhow::Result<()> {
        let committed_path = crate_root().join(COMMITTED_PATH);
        let committed = std::fs::read_to_string(&committed_path).map_err(|e| {
            anyhow::anyhow!("failed to read committed corpus at {committed_path:?}: {e}")
        })?;

        let dir = tempfile::tempdir()?;
        let regenerated_path = dir.path().join("analysis-corpus.json");
        write_to_path(&regenerated_path)?;
        let regenerated = std::fs::read_to_string(&regenerated_path)?;

        assert_eq!(
            committed,
            regenerated,
            "generated analysis corpus is stale — run `mike export-analysis-corpus --out {}` \
             and commit the result",
            committed_path.display()
        );
        Ok(())
    }
}
