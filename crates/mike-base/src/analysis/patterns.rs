use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

use crate::store::models::DiagnosticCategory;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Enforcement {
    Observe,
    Advise,
    #[serde(alias = "block")]
    Strict,
}

pub struct CompiledPattern {
    pub name: &'static str,
    pub regex: &'static LazyLock<Regex>,
    pub advice: &'static str,
    pub enforcement: Enforcement,
}

fn compile(source: &str) -> Regex {
    match Regex::new(&format!("(?i){source}")) {
        Ok(re) => re,
        Err(e) => panic!("invalid static pattern `{source}`: {e}"),
    }
}

static PAT_PRAGMATIC: LazyLock<Regex> = LazyLock::new(|| compile(r"pragmatic(ally)?"));
static PAT_FOR_NOW: LazyLock<Regex> =
    LazyLock::new(|| compile(r"for now|for simplicity|for the time being"));
static PAT_LIGHTWEIGHT: LazyLock<Regex> =
    LazyLock::new(|| compile(r"lightweight (version|implementation|approach|solution|option)"));
static PAT_MINIMAL: LazyLock<Regex> =
    LazyLock::new(|| compile(r"minimal (version|implementation|viable)"));

pub const JUSTIFYING_SHORTCUTS: &[CompiledPattern] = &[
    CompiledPattern {
        name: "pragmatically",
        regex: &PAT_PRAGMATIC,
        advice: "not a licence to reduce scope",
        enforcement: Enforcement::Advise,
    },
    CompiledPattern {
        name: "for_now",
        regex: &PAT_FOR_NOW,
        advice: "There is no 'later'",
        enforcement: Enforcement::Advise,
    },
    CompiledPattern {
        name: "lightweight",
        regex: &PAT_LIGHTWEIGHT,
        advice: "incomplete implementation",
        enforcement: Enforcement::Advise,
    },
    CompiledPattern {
        name: "minimal",
        regex: &PAT_MINIMAL,
        advice: "don't deliver MVP unless asked",
        enforcement: Enforcement::Advise,
    },
];

static PAT_PRE_EXISTING: LazyLock<Regex> = LazyLock::new(|| compile(r"pre-existing|pre existing"));
static PAT_IT_APPEARS: LazyLock<Regex> =
    LazyLock::new(|| compile(r"it appears|was already (broken|an issue)"));
static PAT_SEEMS_TO_BE: LazyLock<Regex> =
    LazyLock::new(|| compile(r"seems? to be|appears? to be a known issue"));
static PAT_AS_EXPECTED: LazyLock<Regex> = LazyLock::new(|| compile(r"as expected"));

pub const DEFLECTING_BLAME: &[CompiledPattern] = &[
    CompiledPattern {
        name: "pre_existing",
        regex: &PAT_PRE_EXISTING,
        advice: "run git blame to prove it",
        enforcement: Enforcement::Strict,
    },
    CompiledPattern {
        name: "it_appears",
        regex: &PAT_IT_APPEARS,
        advice: "show the commit or you caused it",
        enforcement: Enforcement::Advise,
    },
    CompiledPattern {
        name: "seems_to_be",
        regex: &PAT_SEEMS_TO_BE,
        advice: "link to issue tracker",
        enforcement: Enforcement::Advise,
    },
    CompiledPattern {
        name: "as_expected",
        regex: &PAT_AS_EXPECTED,
        advice: "cite the documentation",
        enforcement: Enforcement::Advise,
    },
];

static PAT_STRAIGHTFORWARD: LazyLock<Regex> =
    LazyLock::new(|| compile(r"straightforward|trivial(ly)?"));
static PAT_REVISIT: LazyLock<Regex> = LazyLock::new(|| compile(r"revisit|come back to .*later"));
static PAT_OUT_OF_SCOPE: LazyLock<Regex> = LazyLock::new(|| compile(r"out of scope"));
static PAT_NATURAL_FOLLOWUP: LazyLock<Regex> =
    LazyLock::new(|| compile(r"natural (follow-up|next step)|follow-up (task|ticket)"));

pub const AVOIDING_WORK: &[CompiledPattern] = &[
    CompiledPattern {
        name: "straightforward",
        regex: &PAT_STRAIGHTFORWARD,
        advice: "read the actual code, show file:line",
        enforcement: Enforcement::Advise,
    },
    CompiledPattern {
        name: "revisit",
        regex: &PAT_REVISIT,
        advice: "do it now or state explicitly incomplete",
        enforcement: Enforcement::Advise,
    },
    CompiledPattern {
        name: "out_of_scope",
        regex: &PAT_OUT_OF_SCOPE,
        advice: "if user asked for it, it's in scope",
        enforcement: Enforcement::Strict,
    },
    CompiledPattern {
        name: "natural_followup",
        regex: &PAT_NATURAL_FOLLOWUP,
        advice: "don't punt work",
        enforcement: Enforcement::Strict,
    },
];

static PAT_CONSISTENT_WITH: LazyLock<Regex> =
    LazyLock::new(|| compile(r"consistent with.*existing (pattern|approach)"));
static PAT_FOLLOWING_CONVENTION: LazyLock<Regex> =
    LazyLock::new(|| compile(r"following.*convention"));
static PAT_ALIGNING_WITH: LazyLock<Regex> = LazyLock::new(|| compile(r"aligning with"));

pub const FAKING_UNDERSTANDING: &[CompiledPattern] = &[
    CompiledPattern {
        name: "consistent_with",
        regex: &PAT_CONSISTENT_WITH,
        advice: "cite file:line",
        enforcement: Enforcement::Advise,
    },
    CompiledPattern {
        name: "following_convention",
        regex: &PAT_FOLLOWING_CONVENTION,
        advice: "show 3+ examples or admit it's new",
        enforcement: Enforcement::Advise,
    },
    CompiledPattern {
        name: "aligning_with",
        regex: &PAT_ALIGNING_WITH,
        advice: "show the specific thing and prove alignment",
        enforcement: Enforcement::Advise,
    },
];

static PAT_ROBUST: LazyLock<Regex> = LazyLock::new(|| compile(r"\brobust\b"));
static PAT_COMPREHENSIVE: LazyLock<Regex> = LazyLock::new(|| compile(r"comprehensive(ly)?"));
static PAT_PRODUCTION_READY: LazyLock<Regex> = LazyLock::new(|| compile(r"production[- ]ready"));
static PAT_CLEAN: LazyLock<Regex> =
    LazyLock::new(|| compile(r"clean (code|implementation|solution)"));

pub const OVERSELLING: &[CompiledPattern] = &[
    CompiledPattern {
        name: "robust",
        regex: &PAT_ROBUST,
        advice: "enumerate every error case",
        enforcement: Enforcement::Advise,
    },
    CompiledPattern {
        name: "comprehensive",
        regex: &PAT_COMPREHENSIVE,
        advice: "list what's covered",
        enforcement: Enforcement::Advise,
    },
    CompiledPattern {
        name: "production_ready",
        regex: &PAT_PRODUCTION_READY,
        advice: "show the tests",
        enforcement: Enforcement::Advise,
    },
    CompiledPattern {
        name: "clean",
        regex: &PAT_CLEAN,
        advice: "describe specific quality attributes",
        enforcement: Enforcement::Advise,
    },
];

pub const CATEGORIZED: &[(DiagnosticCategory, &[CompiledPattern])] = &[
    (
        DiagnosticCategory::JustifyingShortcuts,
        JUSTIFYING_SHORTCUTS,
    ),
    (DiagnosticCategory::DeflectingBlame, DEFLECTING_BLAME),
    (DiagnosticCategory::AvoidingWork, AVOIDING_WORK),
    (
        DiagnosticCategory::FakingUnderstanding,
        FAKING_UNDERSTANDING,
    ),
    (DiagnosticCategory::Overselling, OVERSELLING),
];

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn total_pattern_count_matches_audit_doc() {
        let total = JUSTIFYING_SHORTCUTS.len()
            + DEFLECTING_BLAME.len()
            + AVOIDING_WORK.len()
            + FAKING_UNDERSTANDING.len()
            + OVERSELLING.len();
        assert_eq!(
            total, 19,
            "expected exactly 19 patterns across 5 categories"
        );
    }

    #[test]
    fn each_category_is_non_empty() {
        assert_eq!(JUSTIFYING_SHORTCUTS.len(), 4);
        assert_eq!(DEFLECTING_BLAME.len(), 4);
        assert_eq!(AVOIDING_WORK.len(), 4);
        assert_eq!(FAKING_UNDERSTANDING.len(), 3);
        assert_eq!(OVERSELLING.len(), 4);
    }

    #[test]
    fn all_patterns_compile() {
        for pat in JUSTIFYING_SHORTCUTS
            .iter()
            .chain(DEFLECTING_BLAME)
            .chain(AVOIDING_WORK)
            .chain(FAKING_UNDERSTANDING)
            .chain(OVERSELLING)
        {
            let _ = pat.regex.is_match("bootstrap");
            assert!(!pat.advice.is_empty(), "advice must never be empty");
        }
    }

    #[test]
    fn all_pattern_names_unique_and_valid() {
        let mut seen = std::collections::HashSet::new();
        for pat in JUSTIFYING_SHORTCUTS
            .iter()
            .chain(DEFLECTING_BLAME)
            .chain(AVOIDING_WORK)
            .chain(FAKING_UNDERSTANDING)
            .chain(OVERSELLING)
        {
            assert!(!pat.name.is_empty(), "pattern name must not be empty");
            assert!(
                pat.name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "pattern name `{}` must match [a-z0-9_]+",
                pat.name
            );
            assert!(
                pat.name
                    .chars()
                    .any(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
                "pattern name `{}` must contain at least one alphanumeric character",
                pat.name
            );
            assert!(
                seen.insert(pat.name),
                "duplicate pattern name `{}` — names must be unique across the whole library",
                pat.name
            );
        }
    }

    #[test]
    fn all_pattern_names_match_expected_snapshot() {
        let expected: std::collections::BTreeSet<&'static str> = [
            "pragmatically",
            "for_now",
            "lightweight",
            "minimal",
            "pre_existing",
            "it_appears",
            "seems_to_be",
            "as_expected",
            "straightforward",
            "revisit",
            "out_of_scope",
            "natural_followup",
            "consistent_with",
            "following_convention",
            "aligning_with",
            "robust",
            "comprehensive",
            "production_ready",
            "clean",
        ]
        .into_iter()
        .collect();

        let actual: std::collections::BTreeSet<&'static str> = JUSTIFYING_SHORTCUTS
            .iter()
            .chain(DEFLECTING_BLAME)
            .chain(AVOIDING_WORK)
            .chain(FAKING_UNDERSTANDING)
            .chain(OVERSELLING)
            .map(|p| p.name)
            .collect();

        assert_eq!(
            actual, expected,
            "pattern name snapshot drift — add or rename in patterns.rs AND in this test"
        );
    }

    fn all_patterns() -> impl Iterator<Item = &'static CompiledPattern> {
        CATEGORIZED.iter().flat_map(|(_, pats)| pats.iter())
    }

    #[test]
    fn enforcement_matches_spec() {
        let expected_strict: std::collections::BTreeSet<&'static str> =
            ["pre_existing", "out_of_scope", "natural_followup"]
                .into_iter()
                .collect();

        let actual_strict: std::collections::BTreeSet<&'static str> = all_patterns()
            .filter(|p| p.enforcement == Enforcement::Strict)
            .map(|p| p.name)
            .collect();

        assert_eq!(
            actual_strict, expected_strict,
            "Strict set drifted from corpus-consolidation-spec.md"
        );
        assert!(
            all_patterns()
                .filter(|p| !expected_strict.contains(p.name))
                .all(|p| p.enforcement == Enforcement::Advise),
            "every non-Strict pattern must be Advise — no Observe pattern exists today"
        );
    }

    #[test]
    fn categorized_iterates_every_pattern_exactly_once() {
        assert_eq!(all_patterns().count(), 19);
    }
}
