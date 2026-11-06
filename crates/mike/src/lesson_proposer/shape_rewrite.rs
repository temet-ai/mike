use std::fmt::Write as _;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::lesson_shape::assess;
use crate::llm::{CallUsage, LlmError, OpenRouterClient};
use crate::store::persistence::lessons::shape::ShapeWork;

pub const SHAPE_PROMPT: &str = include_str!("prompt/shape.md");
pub const MAX_RULES: usize = 4;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RewriteAnswer {
    #[serde(default)]
    pub rules: Vec<String>,
    #[serde(default)]
    pub reason: String,
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "rules": {"type": "array", "items": {"type": "string"}},
            "reason": {"type": "string"},
        },
        "required": ["rules", "reason"],
    })
}

pub fn build_input(work: &ShapeWork, max_chars: usize) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "## CHARACTER LIMIT PER RULE\n{max_chars}\n");
    let _ = writeln!(out, "## LESSON KIND\n{}\n", work.kind);
    if let Some(topic) = work.topic.as_deref() {
        let _ = writeln!(out, "## LESSON TOPIC\n{topic}\n");
    }
    let _ = writeln!(out, "## WHY IT WAS FLAGGED\n{}\n", work.detail);
    let _ = writeln!(out, "## LESSON TEXT\n{}", work.text);
    out
}

pub async fn call(
    llm: &OpenRouterClient,
    work: &ShapeWork,
    max_chars: usize,
) -> Result<(RewriteAnswer, CallUsage), LlmError> {
    llm.verdict_with_usage::<RewriteAnswer>(SHAPE_PROMPT, &build_input(work, max_chars), schema())
        .await
}

pub fn accepted_rules(rules: &[String], max_chars: usize) -> Result<Vec<String>, String> {
    let mut accepted: Vec<String> = Vec::new();
    for rule in rules {
        let trimmed = rule.trim();
        if trimmed.is_empty() || accepted.iter().any(|seen| seen == trimmed) {
            continue;
        }
        let found = assess(trimmed, None, max_chars);
        if !found.is_rule() {
            return Err(format!(
                "the rewritten rule {trimmed:?} is still not a rule: {}",
                found.detail
            ));
        }
        accepted.push(trimmed.to_owned());
    }
    if accepted.len() > MAX_RULES {
        return Err(format!(
            "the rewrite returned {} rules and the limit is {MAX_RULES}",
            accepted.len()
        ));
    }
    Ok(accepted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work() -> ShapeWork {
        ShapeWork {
            lesson_id: 7,
            text: "Always run the tests. ".repeat(40),
            topic: Some("testing".to_owned()),
            shape: "over_length".to_owned(),
            detail: "the text is 880 characters and the limit is 500".to_owned(),
            attempts: 0,
            kind: "convention".to_owned(),
            stage: "long_term".to_owned(),
            source: "operator".to_owned(),
            project_id: Some(1),
            session_id: None,
            session_uuid: None,
        }
    }

    #[test]
    fn input_carries_the_limit_the_kind_the_topic_and_the_full_text() {
        let work = work();
        let input = build_input(&work, 500);
        assert!(input.contains("## CHARACTER LIMIT PER RULE\n500"));
        assert!(input.contains("convention"));
        assert!(input.contains("testing"));
        assert!(input.contains(&work.text));
    }

    #[test]
    fn accepted_rules_trims_dedupes_and_drops_blanks() -> Result<(), String> {
        let rules = vec![
            "  Run scripts/gate lint before committing.  ".to_owned(),
            "Run scripts/gate lint before committing.".to_owned(),
            "   ".to_owned(),
        ];
        assert_eq!(
            accepted_rules(&rules, 500)?,
            vec!["Run scripts/gate lint before committing.".to_owned()]
        );
        Ok(())
    }

    #[test]
    fn accepted_rules_rejects_a_rule_over_the_limit_and_names_it() {
        let long = "Check every claim against the running system. ".repeat(20);
        let Err(message) = accepted_rules(&[long], 500) else {
            panic!("an over-length rule must be rejected");
        };
        assert!(message.contains("still not a rule"), "{message}");
        assert!(message.contains("limit is 500"), "{message}");
    }

    #[test]
    fn accepted_rules_rejects_a_rule_that_opens_like_a_status_report() {
        let Err(message) = accepted_rules(&["Done. The gate passed.".to_owned()], 500) else {
            panic!("a status-shaped rule must be rejected");
        };
        assert!(message.contains("status word"), "{message}");
    }

    #[test]
    fn accepted_rules_rejects_more_rules_than_the_cap() {
        let rules: Vec<String> = (0..5)
            .map(|n| format!("Run check number {n} before committing."))
            .collect();
        let Err(message) = accepted_rules(&rules, 500) else {
            panic!("five rules must be rejected");
        };
        assert!(message.contains("5 rules"), "{message}");
    }

    #[test]
    fn accepted_rules_returns_empty_for_an_empty_answer() -> Result<(), String> {
        assert!(accepted_rules(&[], 500)?.is_empty());
        Ok(())
    }

    #[test]
    fn answer_defaults_missing_fields() -> Result<(), serde_json::Error> {
        let answer: RewriteAnswer = serde_json::from_str("{}")?;
        assert!(answer.rules.is_empty());
        assert!(answer.reason.is_empty());
        Ok(())
    }
}
