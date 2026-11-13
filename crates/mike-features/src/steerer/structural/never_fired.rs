use std::collections::HashSet;

pub struct NeverFiredRule {
    pub name: &'static str,
    pub declared: &'static [&'static str],
}

pub const CONTEXT_ITEM_KIND: NeverFiredRule = NeverFiredRule {
    name: "mailbox context-item kind",
    declared: &[
        "request", "session", "thread", "message", "commit", "file", "issue", "url", "note",
    ],
};

pub struct NeverFiredFinding {
    pub rule_name: &'static str,
    pub never_observed: Vec<&'static str>,
    pub total_observations: i64,
}

fn diff_never_fired(
    rule: &NeverFiredRule,
    observed: &[String],
    total_observations: i64,
) -> NeverFiredFinding {
    let observed_set: HashSet<&str> = observed.iter().map(|s| s.as_str()).collect();
    let never_observed = rule
        .declared
        .iter()
        .copied()
        .filter(|kind| !observed_set.contains(kind))
        .collect();
    NeverFiredFinding {
        rule_name: rule.name,
        never_observed,
        total_observations,
    }
}

pub async fn check_never_fired_context_kind(
    pool: &sqlx::PgPool,
    rule: &NeverFiredRule,
) -> Result<NeverFiredFinding, sqlx::Error> {
    let observed: Vec<String> = sqlx::query_scalar!(
        r#"
        SELECT DISTINCT elem ->> 'kind' AS "kind!"
          FROM tools.uses tu
          CROSS JOIN LATERAL jsonb_array_elements(tu.input -> 'context') AS elem
         WHERE tu.tool_name ILIKE '%send_session_message%'
           AND jsonb_typeof(tu.input -> 'context') = 'array'
        "#
    )
    .fetch_all(pool)
    .await?;

    let total_observations = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM tools.uses WHERE tool_name ILIKE '%send_session_message%'"#
    )
    .fetch_one(pool)
    .await?;

    Ok(diff_never_fired(rule, &observed, total_observations))
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn never_fired_diff_flags_a_declared_value_absent_from_observed() {
        let rule = NeverFiredRule {
            name: "test rule",
            declared: &["a", "b", "c"],
        };
        let observed = vec!["a".to_owned(), "b".to_owned()];
        let finding = diff_never_fired(&rule, &observed, 42);
        assert_eq!(finding.never_observed, vec!["c"]);
        assert_eq!(finding.total_observations, 42);
    }

    #[test]
    fn never_fired_diff_reports_clean_when_every_declared_value_was_observed() {
        let rule = NeverFiredRule {
            name: "test rule",
            declared: &["a", "b"],
        };
        let observed = vec!["b".to_owned(), "a".to_owned(), "a".to_owned()];
        let finding = diff_never_fired(&rule, &observed, 3);
        assert!(finding.never_observed.is_empty());
    }
}
