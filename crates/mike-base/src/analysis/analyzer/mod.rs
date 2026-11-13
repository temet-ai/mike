use std::collections::{HashMap, HashSet};

use chrono::Utc;
use regex::{Regex, RegexBuilder};

use self::sentence::sentence_matches_any_filter;
use super::config::{AnalysisConfig, CategoryConfig};
use super::patterns::{CompiledPattern, Enforcement};
use crate::store::models::{Diagnostic, DiagnosticCategory};

#[cfg(test)]
pub mod fp_filter_tests;
#[cfg(test)]
pub mod override_tests;
#[cfg(test)]
pub mod registry_tests;
pub mod sentence;

const OPERATOR_REGEX_SIZE_LIMIT: usize = 1_000_000;

const OPERATOR_REGEX_DFA_SIZE_LIMIT: usize = 1_000_000;

pub trait Analyzer: Send + Sync {
    fn name(&self) -> &'static str;

    fn category(&self) -> DiagnosticCategory;

    fn analyze(&self, text: &str) -> Option<Finding>;
}

pub struct Finding {
    pub pattern_name: &'static str,
    pub matched_text: String,
    pub match_range: (usize, usize),
    pub other_hits: usize,
    pub advice: String,
}

struct CompiledCategoryFilters {
    enabled: bool,
    suppress_if_sentence_matches: Vec<Regex>,
}

impl CompiledCategoryFilters {
    fn compile(cat_name: &str, cfg: &CategoryConfig) -> anyhow::Result<Self> {
        let mut suppress = Vec::with_capacity(cfg.false_positive_filters.len());
        for source in &cfg.false_positive_filters {
            let re = RegexBuilder::new(source)
                .case_insensitive(true)
                .size_limit(OPERATOR_REGEX_SIZE_LIMIT)
                .dfa_size_limit(OPERATOR_REGEX_DFA_SIZE_LIMIT)
                .build()
                .map_err(|e| {
                    anyhow::anyhow!(
                        "invalid false-positive filter in category `{cat_name}` with source `{source}`: {e}"
                    )
                })?;
            suppress.push(re);
        }
        Ok(Self {
            enabled: cfg.enabled,
            suppress_if_sentence_matches: suppress,
        })
    }

    fn default_enabled() -> Self {
        Self {
            enabled: true,
            suppress_if_sentence_matches: Vec::new(),
        }
    }
}

pub struct AnalyzerRegistry {
    entries: Vec<RegistryEntry>,
    enabled: bool,
    advice_overrides: HashMap<String, String>,
    enforcement_overrides: HashMap<String, Enforcement>,
}

struct RegistryEntry {
    analyzer: Box<dyn Analyzer>,
    filters: CompiledCategoryFilters,
}

pub struct AnalyzerSnapshot {
    pub name: &'static str,
    pub category: DiagnosticCategory,
    pub enabled: bool,
}

fn build_registry_entries(
    all: Vec<Box<dyn Analyzer>>,
    config: &AnalysisConfig,
) -> anyhow::Result<Vec<RegistryEntry>> {
    let mut entries = Vec::with_capacity(all.len());
    for analyzer in all {
        let cat = analyzer.category();
        let filters = match config.categories.get(&cat) {
            Some(cfg) => {
                let cat_name = category_name(&cat);
                CompiledCategoryFilters::compile(cat_name, cfg)?
            }
            None => CompiledCategoryFilters::default_enabled(),
        };
        entries.push(RegistryEntry { analyzer, filters });
    }
    Ok(entries)
}

fn log_unknown_pattern_override(name: &str) {
    tracing::debug!(pattern = %name, "per-pattern override targets unknown pattern — ignoring");
}

fn log_empty_advice_override(name: &str) {
    tracing::debug!(
        pattern = %name,
        "per-pattern advice override is empty or whitespace — ignoring"
    );
}

fn apply_pattern_override(
    name: &str,
    cfg: &super::config::PatternConfig,
    known_names: &HashSet<&'static str>,
    advice_overrides: &mut HashMap<String, String>,
    enforcement_overrides: &mut HashMap<String, Enforcement>,
) {
    if !known_names.contains(name) {
        if cfg.advice.is_some() || cfg.enforcement.is_some() {
            log_unknown_pattern_override(name);
        }
        return;
    }
    if let Some(advice) = cfg.advice.as_deref() {
        let advice = advice.trim();
        if advice.is_empty() {
            log_empty_advice_override(name);
        } else {
            advice_overrides.insert(name.to_string(), advice.to_string());
        }
    }
    if let Some(enforcement) = cfg.enforcement {
        enforcement_overrides.insert(name.to_string(), enforcement);
    }
}

fn collect_pattern_overrides(
    config: &AnalysisConfig,
    known_names: &HashSet<&'static str>,
) -> (HashMap<String, String>, HashMap<String, Enforcement>) {
    let mut advice_overrides: HashMap<String, String> = HashMap::new();
    let mut enforcement_overrides: HashMap<String, Enforcement> = HashMap::new();
    for (name, cfg) in &config.patterns {
        apply_pattern_override(
            name,
            cfg,
            known_names,
            &mut advice_overrides,
            &mut enforcement_overrides,
        );
    }
    (advice_overrides, enforcement_overrides)
}

impl AnalyzerRegistry {
    pub fn with_defaults(config: &AnalysisConfig) -> anyhow::Result<Self> {
        use super::categories::{
            AvoidingWorkAnalyzer, DeflectingBlameAnalyzer, FakingUnderstandingAnalyzer,
            JustifyingShortcutsAnalyzer, OversellingAnalyzer,
        };

        let all: Vec<Box<dyn Analyzer>> = vec![
            Box::new(JustifyingShortcutsAnalyzer),
            Box::new(DeflectingBlameAnalyzer),
            Box::new(AvoidingWorkAnalyzer),
            Box::new(FakingUnderstandingAnalyzer),
            Box::new(OversellingAnalyzer),
        ];
        let entries = build_registry_entries(all, config)?;

        let known_names = collect_known_pattern_names();
        let (advice_overrides, enforcement_overrides) =
            collect_pattern_overrides(config, &known_names);

        Ok(Self {
            entries,
            enabled: config.enabled,
            advice_overrides,
            enforcement_overrides,
        })
    }

    pub fn patterns_by_enforcement(
        &self,
        class: Enforcement,
    ) -> Vec<&'static CompiledPattern> {
        super::patterns::CATEGORIZED
            .iter()
            .flat_map(|(_, pats)| pats.iter())
            .filter(|pat| {
                self.enforcement_overrides
                    .get(pat.name)
                    .copied()
                    .unwrap_or(pat.enforcement)
                    == class
            })
            .collect()
    }

    pub fn advice_overrides(&self) -> &HashMap<String, String> {
        &self.advice_overrides
    }

    pub fn enforcement_overrides(&self) -> &HashMap<String, Enforcement> {
        &self.enforcement_overrides
    }

    pub fn run(&self, text: &str) -> Vec<Diagnostic> {
        if !self.enabled || text.is_empty() {
            return Vec::new();
        }

        let now = Utc::now();
        let mut out = Vec::new();
        for entry in &self.entries {
            if !entry.filters.enabled {
                continue;
            }
            let Some(finding) = entry.analyzer.analyze(text) else {
                continue;
            };
            if sentence_matches_any_filter(
                text,
                finding.match_range,
                &entry.filters.suppress_if_sentence_matches,
            ) {
                continue;
            }
            let advice = self
                .advice_overrides
                .get(finding.pattern_name)
                .cloned()
                .unwrap_or(finding.advice);
            out.push(Diagnostic {
                category: entry.analyzer.category(),
                analyzer: entry.analyzer.name().to_string(),
                advice,
                matched_text: finding.matched_text,
                match_range: finding.match_range,
                other_hits: finding.other_hits,
                created_at: now,
            });
        }
        out
    }

    pub fn snapshot(&self) -> Vec<AnalyzerSnapshot> {
        self.entries
            .iter()
            .map(|entry| AnalyzerSnapshot {
                name: entry.analyzer.name(),
                category: entry.analyzer.category(),
                enabled: self.enabled && entry.filters.enabled,
            })
            .collect()
    }

    pub fn snapshot_with_overrides(
        &self,
        overrides: &HashMap<String, bool>,
    ) -> Vec<AnalyzerSnapshot> {
        self.snapshot()
            .into_iter()
            .map(|mut row| {
                if let Some(&enabled) = overrides.get(row.name) {
                    row.enabled = enabled;
                }
                row
            })
            .collect()
    }
}

pub fn analyzer_display_name(category: &DiagnosticCategory) -> &'static str {
    match category {
        DiagnosticCategory::JustifyingShortcuts => "Justifying shortcuts",
        DiagnosticCategory::DeflectingBlame => "Deflecting blame",
        DiagnosticCategory::AvoidingWork => "Avoiding work",
        DiagnosticCategory::FakingUnderstanding => "Faking understanding",
        DiagnosticCategory::Overselling => "Overselling",
    }
}

fn category_name(cat: &DiagnosticCategory) -> &'static str {
    match cat {
        DiagnosticCategory::JustifyingShortcuts => "justifying_shortcuts",
        DiagnosticCategory::DeflectingBlame => "deflecting_blame",
        DiagnosticCategory::AvoidingWork => "avoiding_work",
        DiagnosticCategory::FakingUnderstanding => "faking_understanding",
        DiagnosticCategory::Overselling => "overselling",
    }
}

fn collect_known_pattern_names() -> HashSet<&'static str> {
    use super::patterns::{
        AVOIDING_WORK, DEFLECTING_BLAME, FAKING_UNDERSTANDING, JUSTIFYING_SHORTCUTS, OVERSELLING,
    };
    let mut set = HashSet::new();
    for pat in JUSTIFYING_SHORTCUTS
        .iter()
        .chain(DEFLECTING_BLAME)
        .chain(AVOIDING_WORK)
        .chain(FAKING_UNDERSTANDING)
        .chain(OVERSELLING)
    {
        set.insert(pat.name);
    }
    set
}
