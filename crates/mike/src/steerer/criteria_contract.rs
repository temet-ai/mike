#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CriterionVerdict {
    Accepted,
    Rejected(&'static str),
}

const IMPERATIVE_OPENINGS: &[&str] = &[
    "add",
    "create",
    "build",
    "implement",
    "write",
    "make",
    "update",
    "read",
    "run",
    "call",
    "invoke",
    "open",
    "follow",
    "change",
    "modify",
    "edit",
    "fix",
    "refactor",
    "rework",
    "rename",
    "remove",
    "delete",
    "drop",
    "wire",
    "hook",
    "integrate",
    "introduce",
    "support",
    "handle",
    "improve",
    "clean",
    "tidy",
    "port",
    "migrate",
    "extend",
    "expose",
    "surface",
    "ship",
    "land",
    "apply",
    "use",
    "set",
];

const VERIFICATION_OPENINGS: &[&str] = &[
    "ensure",
    "verify",
    "confirm",
    "check",
    "prove",
    "show",
    "demonstrate",
];

pub(crate) fn classify(criterion: &str) -> CriterionVerdict {
    let text = strip_list_marker(criterion.trim());
    if text.is_empty() {
        return CriterionVerdict::Rejected("a criterion cannot be empty");
    }

    let Some(first) = first_word_lowercase(text) else {
        return CriterionVerdict::Rejected("a criterion cannot be empty");
    };

    if VERIFICATION_OPENINGS.contains(&first.as_str()) {
        return CriterionVerdict::Accepted;
    }

    if IMPERATIVE_OPENINGS.contains(&first.as_str()) {
        return CriterionVerdict::Rejected(
            "this commands a change instead of describing the state of the surface \
             afterwards — say what is true once the work is done, not what to do",
        );
    }

    CriterionVerdict::Accepted
}

fn strip_list_marker(text: &str) -> &str {
    let mut rest = text;
    if let Some(stripped) = rest
        .strip_prefix("- ")
        .or_else(|| rest.strip_prefix("* "))
        .or_else(|| rest.strip_prefix("+ "))
    {
        rest = stripped.trim_start();
    } else {
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if !digits.is_empty() {
            let after = &rest[digits.len()..];
            if let Some(stripped) = after
                .strip_prefix(". ")
                .or_else(|| after.strip_prefix(") "))
            {
                rest = stripped.trim_start();
            }
        }
    }
    if rest.starts_with('[')
        && let Some(close) = rest.find(']').filter(|c| *c <= 3)
    {
        rest = rest[close + 1..].trim_start();
    }
    rest
}

fn first_word_lowercase(text: &str) -> Option<String> {
    let word: String = text
        .split_whitespace()
        .next()?
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase();
    if word.is_empty() { None } else { Some(word) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rejected(text: &str) -> bool {
        matches!(classify(text), CriterionVerdict::Rejected(_))
    }

    #[test]
    fn the_comms_example_flips() {
        assert!(
            rejected("Add a comms panel to the session page."),
            "the artifact-scoped criterion the shipped comms feature satisfied must be rejected"
        );
        assert_eq!(
            classify(
                "The comms panel is reachable from the session page without a direct URL, is \
                 built on the current shared primitives, renders in place alongside its \
                 neighbours without disturbing them, and every branch it introduces is \
                 exercised by a real session."
            ),
            CriterionVerdict::Accepted,
            "the surface-scoped rewrite must be accepted"
        );
    }

    #[test]
    fn the_usual_ways_of_writing_a_criterion_are_all_artifact_scoped() {
        for artifact_scoped in [
            "Read the style guide and apply the change",
            "Update the component",
            "Add X",
            "create the migration",
            "Implement the retry",
            "wire the panel into the page",
            "Refactor bodyFetcher",
            "Migrate the callers off request_body",
        ] {
            assert!(
                rejected(artifact_scoped),
                "{artifact_scoped:?} names the change, not the end state — it must be rejected"
            );
        }
    }

    #[test]
    fn statements_about_the_surface_are_accepted() {
        for surface_scoped in [
            "The session page shows the comms panel without needing a direct URL.",
            "Every branch introduced by the retry path is exercised by a real captured session.",
            "`bodyFetcher` reads the persisted byte count rather than the raw body.",
            "No page can be reached whose components are not on the current shared set.",
            "The migration's callers still compile and their queries measure within budget.",
        ] {
            assert_eq!(
                classify(surface_scoped),
                CriterionVerdict::Accepted,
                "{surface_scoped:?} describes a checkable state and must be accepted"
            );
        }
    }

    #[test]
    fn verification_openings_survive() {
        for verifying in [
            "Ensure the panel is reachable from the session page.",
            "Verify the rebuilt body carries the same JSON value.",
            "Confirm no branch is left unexercised.",
        ] {
            assert_eq!(
                classify(verifying),
                CriterionVerdict::Accepted,
                "{verifying:?}"
            );
        }
    }

    #[test]
    fn list_markers_are_not_part_of_the_sentence() {
        assert!(rejected("- Add a comms panel"));
        assert!(rejected("* Update the component"));
        assert!(rejected("1. Create the migration"));
        assert!(rejected("- [ ] Add a comms panel"));
        assert!(rejected("[x] Implement the retry"));
        assert_eq!(
            classify("- The comms panel is reachable from the session page."),
            CriterionVerdict::Accepted
        );
    }

    #[test]
    fn an_empty_criterion_is_rejected() {
        assert!(rejected(""));
        assert!(rejected("   "));
        assert!(rejected("- "));
    }

    #[test]
    fn only_the_leading_word_decides() {
        assert_eq!(
            classify("Added rows are visible on the session page."),
            CriterionVerdict::Accepted,
            "past tense describes a state"
        );
        assert_eq!(
            classify("Updates to the panel appear without a reload."),
            CriterionVerdict::Accepted,
            "a noun that happens to look like a verb"
        );
        assert_eq!(
            classify("A user can add a comms panel from the session page."),
            CriterionVerdict::Accepted,
            "the imperative is not at the head — this describes a capability"
        );
    }
}
