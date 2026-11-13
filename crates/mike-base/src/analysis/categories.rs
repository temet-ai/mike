use super::analyzer::{Analyzer, Finding};
use super::patterns::{
    AVOIDING_WORK, CompiledPattern, DEFLECTING_BLAME, FAKING_UNDERSTANDING, JUSTIFYING_SHORTCUTS,
    OVERSELLING,
};
use crate::store::models::DiagnosticCategory;

fn collect_first_with_extras(text: &str, patterns: &[CompiledPattern]) -> Option<Finding> {
    #[derive(Clone)]
    struct Candidate {
        start: usize,
        end: usize,
        matched: String,
        advice: &'static str,
        pattern_name: &'static str,
    }

    let mut primary: Option<Candidate> = None;
    let mut extras: usize = 0;

    for pat in patterns {
        for m in pat.regex.find_iter(text) {
            match &primary {
                None => {
                    primary = Some(Candidate {
                        start: m.start(),
                        end: m.end(),
                        matched: m.as_str().to_string(),
                        advice: pat.advice,
                        pattern_name: pat.name,
                    });
                }
                Some(current) => {
                    if m.start() < current.start {
                        extras += 1;
                        primary = Some(Candidate {
                            start: m.start(),
                            end: m.end(),
                            matched: m.as_str().to_string(),
                            advice: pat.advice,
                            pattern_name: pat.name,
                        });
                    } else {
                        extras += 1;
                    }
                }
            }
        }
    }

    primary.map(|c| Finding {
        pattern_name: c.pattern_name,
        matched_text: c.matched,
        match_range: (c.start, c.end),
        other_hits: extras,
        advice: c.advice.to_string(),
    })
}

pub struct JustifyingShortcutsAnalyzer;

impl Analyzer for JustifyingShortcutsAnalyzer {
    fn name(&self) -> &'static str {
        "justifying_shortcuts_analyzer"
    }
    fn category(&self) -> DiagnosticCategory {
        DiagnosticCategory::JustifyingShortcuts
    }
    fn analyze(&self, text: &str) -> Option<Finding> {
        collect_first_with_extras(text, JUSTIFYING_SHORTCUTS)
    }
}

pub struct DeflectingBlameAnalyzer;

impl Analyzer for DeflectingBlameAnalyzer {
    fn name(&self) -> &'static str {
        "deflecting_blame_analyzer"
    }
    fn category(&self) -> DiagnosticCategory {
        DiagnosticCategory::DeflectingBlame
    }
    fn analyze(&self, text: &str) -> Option<Finding> {
        collect_first_with_extras(text, DEFLECTING_BLAME)
    }
}

pub struct AvoidingWorkAnalyzer;

impl Analyzer for AvoidingWorkAnalyzer {
    fn name(&self) -> &'static str {
        "avoiding_work_analyzer"
    }
    fn category(&self) -> DiagnosticCategory {
        DiagnosticCategory::AvoidingWork
    }
    fn analyze(&self, text: &str) -> Option<Finding> {
        collect_first_with_extras(text, AVOIDING_WORK)
    }
}

pub struct FakingUnderstandingAnalyzer;

impl Analyzer for FakingUnderstandingAnalyzer {
    fn name(&self) -> &'static str {
        "faking_understanding_analyzer"
    }
    fn category(&self) -> DiagnosticCategory {
        DiagnosticCategory::FakingUnderstanding
    }
    fn analyze(&self, text: &str) -> Option<Finding> {
        collect_first_with_extras(text, FAKING_UNDERSTANDING)
    }
}

pub struct OversellingAnalyzer;

impl Analyzer for OversellingAnalyzer {
    fn name(&self) -> &'static str {
        "overselling_analyzer"
    }
    fn category(&self) -> DiagnosticCategory {
        DiagnosticCategory::Overselling
    }
    fn analyze(&self, text: &str) -> Option<Finding> {
        collect_first_with_extras(text, OVERSELLING)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn pattern_pragmatic_matches_positive_and_ignores_negative() {
        let a = JustifyingShortcutsAnalyzer;
        assert!(
            a.analyze("We took a pragmatic approach here.").is_some(),
            "pragmatic must fire"
        );
        assert!(
            a.analyze("The program is efficient and correct.").is_none(),
            "neutral text must not fire"
        );
    }

    #[test]
    fn pattern_for_now_matches_positive_and_ignores_negative() {
        let a = JustifyingShortcutsAnalyzer;
        assert!(a.analyze("Using the stub for now.").is_some());
        assert!(a.analyze("We finished the implementation.").is_none());
    }

    #[test]
    fn pattern_lightweight_matches_positive_and_ignores_negative() {
        let a = JustifyingShortcutsAnalyzer;
        assert!(a.analyze("Shipping a lightweight version.").is_some());
        assert!(a.analyze("A heavy-weight build system.").is_none());
    }

    #[test]
    fn pattern_minimal_matches_positive_and_ignores_negative() {
        let a = JustifyingShortcutsAnalyzer;
        assert!(a.analyze("Here's the minimal implementation.").is_some());
        assert!(a.analyze("The sample input is complete.").is_none());
    }

    #[test]
    fn pattern_pre_existing_matches_positive_and_ignores_negative() {
        let a = DeflectingBlameAnalyzer;
        assert!(a.analyze("This is pre-existing behavior.").is_some());
        assert!(a.analyze("The feature is brand new.").is_none());
    }

    #[test]
    fn pattern_it_appears_matches_positive_and_ignores_negative() {
        let a = DeflectingBlameAnalyzer;
        assert!(
            a.analyze("It appears the tests are flaky already.")
                .is_some()
        );
        assert!(a.analyze("Everything compiles cleanly.").is_none());
    }

    #[test]
    fn pattern_seems_to_be_matches_positive_and_ignores_negative() {
        let a = DeflectingBlameAnalyzer;
        assert!(a.analyze("This seems to be happening upstream.").is_some());
        assert!(a.analyze("All assertions pass.").is_none());
    }

    #[test]
    fn pattern_as_expected_matches_positive_and_ignores_negative() {
        let a = DeflectingBlameAnalyzer;
        assert!(a.analyze("The call fails as expected.").is_some());
        assert!(a.analyze("The call succeeded cleanly.").is_none());
    }

    #[test]
    fn pattern_straightforward_matches_positive_and_ignores_negative() {
        let a = AvoidingWorkAnalyzer;
        assert!(
            a.analyze("The fix is straightforward once you see it.")
                .is_some()
        );
        assert!(
            a.analyze("I tested every branch and every boundary.")
                .is_none()
        );
    }

    #[test]
    fn pattern_revisit_matches_positive_and_ignores_negative() {
        let a = AvoidingWorkAnalyzer;
        assert!(a.analyze("Let's revisit this next week.").is_some());
        assert!(a.analyze("I resolved every open issue.").is_none());
    }

    #[test]
    fn pattern_out_of_scope_matches_positive_and_ignores_negative() {
        let a = AvoidingWorkAnalyzer;
        assert!(a.analyze("That change is out of scope.").is_some());
        assert!(a.analyze("Everything requested is done.").is_none());
    }

    #[test]
    fn pattern_natural_followup_matches_positive_and_ignores_negative() {
        let a = AvoidingWorkAnalyzer;
        assert!(a.analyze("A natural follow-up would be tests.").is_some());
        assert!(a.analyze("Tests are included with the change.").is_none());
    }

    #[test]
    fn pattern_consistent_with_matches_positive_and_ignores_negative() {
        let a = FakingUnderstandingAnalyzer;
        assert!(
            a.analyze("This is consistent with the existing pattern.")
                .is_some()
        );
        assert!(
            a.analyze("I rewrote the helper to be purely functional.")
                .is_none()
        );
    }

    #[test]
    fn pattern_following_convention_matches_positive_and_ignores_negative() {
        let a = FakingUnderstandingAnalyzer;
        assert!(a.analyze("Following the project convention.").is_some());
        assert!(
            a.analyze("I introduced a new naming scheme this patch.")
                .is_none()
        );
    }

    #[test]
    fn pattern_aligning_with_matches_positive_and_ignores_negative() {
        let a = FakingUnderstandingAnalyzer;
        assert!(a.analyze("We are aligning with the old API.").is_some());
        assert!(a.analyze("The breaking change is intentional.").is_none());
    }

    #[test]
    fn pattern_robust_matches_positive_and_ignores_negative() {
        let a = OversellingAnalyzer;
        assert!(a.analyze("This is a robust solution.").is_some());
        assert!(a.analyze("I added one unit test.").is_none());
    }

    #[test]
    fn pattern_comprehensive_matches_positive_and_ignores_negative() {
        let a = OversellingAnalyzer;
        assert!(
            a.analyze("Comprehensive coverage of the error path.")
                .is_some()
        );
        assert!(a.analyze("I stubbed the error case.").is_none());
    }

    #[test]
    fn pattern_production_ready_matches_positive_and_ignores_negative() {
        let a = OversellingAnalyzer;
        assert!(a.analyze("This is production-ready.").is_some());
        assert!(a.analyze("This is alpha quality.").is_none());
    }

    #[test]
    fn pattern_clean_matches_positive_and_ignores_negative() {
        let a = OversellingAnalyzer;
        assert!(a.analyze("A clean implementation of retries.").is_some());
        assert!(a.analyze("I added another if-let branch.").is_none());
    }

    #[test]
    fn collect_first_returns_none_when_no_matches() {
        let out = collect_first_with_extras("neutral text only", OVERSELLING);
        assert!(out.is_none());
    }

    #[test]
    fn collect_first_tracks_extras_across_patterns() -> anyhow::Result<()> {
        let out = collect_first_with_extras(
            "pragmatic minimal implementation for now",
            JUSTIFYING_SHORTCUTS,
        );
        let finding = out.ok_or_else(|| anyhow::anyhow!("should fire on trigger text"))?;
        assert_eq!(finding.other_hits, 2, "two extras beyond the first match");
        assert!(!finding.matched_text.is_empty());
        Ok(())
    }
}
