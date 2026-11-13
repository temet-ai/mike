use crate::store::models::Diagnostic;

pub fn build_reminder_text(findings: &[Diagnostic]) -> Option<String> {
    if findings.is_empty() {
        return None;
    }

    let mut out = String::new();
    out.push_str("<system-reminder source=\"proxy-reminders\">\n");
    out.push_str("Your previous response contained anti-patterns flagged by the proxy:\n");
    for diag in findings {
        let matched = sanitize_for_inline(&diag.matched_text);
        let advice = sanitize_for_inline(&diag.advice);
        out.push_str("- \"");
        out.push_str(&matched);
        out.push_str("\" — ");
        out.push_str(&advice);
        out.push('\n');
    }
    out.push_str(
        "Restate with appropriate confidence, or declare `Blocked: <reason>` if uncertain.\n",
    );
    out.push_str("</system-reminder>");
    Some(out)
}

pub fn build_lessons_reminder_text(lessons: &[(String, String)]) -> Option<String> {
    if lessons.is_empty() {
        return None;
    }

    let mut out = String::new();
    out.push_str(LESSON_REMINDER_OPEN_TAG);
    out.push('\n');
    out.push_str("Lessons recorded from prior sessions:\n");
    for (kind, text) in lessons {
        let kind_s = sanitize_for_inline(kind);
        let text_trimmed = text.trim();
        out.push_str("- [");
        out.push_str(&kind_s);
        out.push_str("] ");
        out.push_str(text_trimmed);
        out.push('\n');
    }
    out.push_str(SYSTEM_REMINDER_CLOSE_TAG);
    Some(out)
}

pub const LESSON_REMINDER_OPEN_TAG: &str = "<system-reminder source=\"proxy-lessons\">";

pub const SYSTEM_REMINDER_CLOSE_TAG: &str = "</system-reminder>";

pub fn sanitize_for_inline(s: &str) -> String {
    let trimmed = s.trim();
    let mut out = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        if ch.is_control() {
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::store::models::DiagnosticCategory;
    use chrono::Utc;

    fn mk_diag(matched: &str, advice: &str) -> Diagnostic {
        Diagnostic {
            category: DiagnosticCategory::Overselling,
            analyzer: "test_analyzer".to_string(),
            advice: advice.to_string(),
            matched_text: matched.to_string(),
            match_range: (0, matched.len()),
            other_hits: 0,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn empty_findings_returns_none() {
        assert!(build_reminder_text(&[]).is_none());
    }

    #[test]
    fn single_finding_produces_expected_body() -> anyhow::Result<()> {
        let diags = vec![mk_diag("comprehensive solution", "no MVP without ask")];
        let text = build_reminder_text(&diags)
            .ok_or_else(|| anyhow::anyhow!("expected Some reminder body"))?;
        let expected = "<system-reminder source=\"proxy-reminders\">\n\
                        Your previous response contained anti-patterns flagged by the proxy:\n\
                        - \"comprehensive solution\" — no MVP without ask\n\
                        Restate with appropriate confidence, or declare `Blocked: <reason>` if uncertain.\n\
                        </system-reminder>";
        assert_eq!(text, expected);
        Ok(())
    }

    #[test]
    fn multiple_findings_each_on_their_own_bullet() -> anyhow::Result<()> {
        let diags = vec![
            mk_diag("pragmatically", "not a licence to reduce scope"),
            mk_diag("for now", "There is no 'later'"),
        ];
        let text = build_reminder_text(&diags)
            .ok_or_else(|| anyhow::anyhow!("expected Some reminder body"))?;
        assert!(text.contains("- \"pragmatically\" — not a licence to reduce scope\n"));
        assert!(text.contains("- \"for now\" — There is no 'later'\n"));
        assert_eq!(text.matches("<system-reminder").count(), 1);
        assert_eq!(text.matches("</system-reminder>").count(), 1);
        Ok(())
    }

    #[test]
    fn matched_text_with_newlines_is_inlined_to_spaces() -> anyhow::Result<()> {
        let diags = vec![mk_diag("line1\nline2\nline3", "advice")];
        let text = build_reminder_text(&diags)
            .ok_or_else(|| anyhow::anyhow!("expected Some reminder body"))?;
        assert!(text.contains("- \"line1 line2 line3\" — advice\n"));
        Ok(())
    }

    #[test]
    fn matched_text_with_quotes_is_preserved_verbatim() -> anyhow::Result<()> {
        let diags = vec![mk_diag("the \"clean\" solution", "no")];
        let text = build_reminder_text(&diags)
            .ok_or_else(|| anyhow::anyhow!("expected Some reminder body"))?;
        assert!(text.contains("- \"the \"clean\" solution\" — no\n"));
        Ok(())
    }

    #[test]
    fn matched_text_is_not_truncated_for_long_strings() -> anyhow::Result<()> {
        let long: String = "a".repeat(5_000);
        let diags = vec![mk_diag(&long, "advice")];
        let text = build_reminder_text(&diags)
            .ok_or_else(|| anyhow::anyhow!("expected Some reminder body"))?;
        assert!(text.contains(&long), "matched_text must round-trip in full");
        Ok(())
    }

    #[test]
    fn sanitize_strips_leading_and_trailing_whitespace_only() -> anyhow::Result<()> {
        let diags = vec![mk_diag("  inner space  ", "advice")];
        let text = build_reminder_text(&diags)
            .ok_or_else(|| anyhow::anyhow!("expected Some reminder body"))?;
        assert!(text.contains("- \"inner space\" — advice\n"));
        Ok(())
    }

    #[test]
    fn empty_lessons_returns_none() {
        let result = build_lessons_reminder_text(&[]);
        assert!(result.is_none());
    }

    #[test]
    fn single_correction_lesson_produces_expected_body() -> anyhow::Result<()> {
        let lessons = vec![(
            "correction".to_string(),
            "Always ask before widening scope.".to_string(),
        )];
        let text = build_lessons_reminder_text(&lessons)
            .ok_or_else(|| anyhow::anyhow!("expected Some"))?;
        let expected = "<system-reminder source=\"proxy-lessons\">\n\
                        Lessons recorded from prior sessions:\n\
                        - [correction] Always ask before widening scope.\n\
                        </system-reminder>";
        assert_eq!(text, expected);
        Ok(())
    }

    #[test]
    fn mixed_kinds_all_appear_in_output() -> anyhow::Result<()> {
        let lessons = vec![
            ("correction".to_string(), "Lesson A".to_string()),
            ("convention".to_string(), "Lesson B".to_string()),
            ("warning".to_string(), "Lesson C".to_string()),
        ];
        let text = build_lessons_reminder_text(&lessons)
            .ok_or_else(|| anyhow::anyhow!("expected Some"))?;
        assert!(text.contains("- [correction] Lesson A\n"));
        assert!(text.contains("- [convention] Lesson B\n"));
        assert!(text.contains("- [warning] Lesson C\n"));
        assert_eq!(text.matches("<system-reminder").count(), 1);
        assert_eq!(text.matches("</system-reminder>").count(), 1);
        Ok(())
    }

    #[test]
    fn lesson_text_is_not_truncated() -> anyhow::Result<()> {
        let long_text: String = "x".repeat(5_000);
        let lessons = vec![("preference".to_string(), long_text.clone())];
        let text = build_lessons_reminder_text(&lessons)
            .ok_or_else(|| anyhow::anyhow!("expected Some"))?;
        assert!(
            text.contains(&long_text),
            "lesson text must round-trip in full"
        );
        Ok(())
    }
}
