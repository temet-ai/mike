pub mod budget;
pub mod prompt;

use tracing::{debug, info, warn};

use crate::llm::{LlmError, OpenRouterClient};
use crate::steerer::criteria_contract::{CriterionVerdict, classify};
use crate::store::persistence::acceptance_criteria::{AskSeen, NewCriterion};
use crate::store::persistence::memory_ask_ledger::AskLedgerEntryRow;
pub use budget::IntakeInputBudget;
use prompt::{AuthoredCriterion, authored_criteria_schema, build_user_message};

pub fn should_reauthor(seen: Option<AskSeen>, newest_ask_entry_id: Option<i64>) -> bool {
    let Some(newest) = newest_ask_entry_id else {
        return false;
    };
    match seen {
        None => true,
        Some(AskSeen {
            newest_ask_entry_id: None,
            ..
        }) => true,
        Some(AskSeen {
            newest_ask_entry_id: Some(last_seen),
            ..
        }) => newest > last_seen,
    }
}

fn is_rejected_by_contract(criterion: &str) -> bool {
    match classify(criterion) {
        CriterionVerdict::Rejected(reason) => {
            info!(
                criterion = %criterion,
                reason,
                "intake criteria: rejected an artifact-scoped criterion (#153)"
            );
            true
        }
        CriterionVerdict::Accepted => false,
    }
}

fn resolve_provenance(item: &AuthoredCriterion) -> Option<(&'static str, Option<String>)> {
    let span = item
        .operator_span
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    match item.provenance.as_str() {
        "operator" if span.is_some() => Some(("operator", span)),
        "operator" => Some(demote_unquoted_operator_claim(item)),
        "inferred" => Some(("inferred", None)),
        other => drop_unrecognised_provenance(item, other),
    }
}

fn demote_unquoted_operator_claim(item: &AuthoredCriterion) -> (&'static str, Option<String>) {
    warn!(
        criterion = %item.criterion,
        "intake criteria: demoting an unquoted `operator` criterion to `inferred`"
    );
    ("inferred", None)
}

fn drop_unrecognised_provenance(
    item: &AuthoredCriterion,
    provenance: &str,
) -> Option<(&'static str, Option<String>)> {
    warn!(
        criterion = %item.criterion,
        provenance,
        "intake criteria: dropping a criterion with an unrecognised provenance"
    );
    None
}

pub fn apply_contract(authored: Vec<AuthoredCriterion>) -> Vec<NewCriterion> {
    let mut kept = Vec::with_capacity(authored.len());
    for item in authored {
        if is_rejected_by_contract(&item.criterion) {
            continue;
        }
        let Some((provenance, operator_span)) = resolve_provenance(&item) else {
            continue;
        };
        kept.push(NewCriterion {
            criterion: item.criterion.trim().to_owned(),
            provenance: provenance.to_owned(),
            operator_span,
        });
    }
    kept
}

fn log_authoring_failure(e: &LlmError) {
    if e.is_refusal() {
        log_authoring_refusal(e);
    } else {
        log_authoring_error(e);
    }
}

fn log_authoring_refusal(e: &LlmError) {
    debug!(error = %e, "intake criteria: call governor refused — no map this turn");
}

fn log_authoring_error(e: &LlmError) {
    warn!(error = %e, "intake criteria: authoring failed — session keeps its previous map");
}

pub async fn author(
    llm: &OpenRouterClient,
    entries: &[AskLedgerEntryRow],
    older_not_fetched: usize,
    project_path: &str,
    budget: IntakeInputBudget,
) -> Option<Vec<NewCriterion>> {
    if entries.is_empty() {
        return None;
    }
    let user = build_user_message(entries, older_not_fetched, project_path, budget);
    let authored: prompt::AuthoredCriteria = match llm
        .verdict(prompt::SYSTEM_PROMPT, &user, authored_criteria_schema())
        .await
    {
        Ok(v) => v,
        Err(e) => {
            log_authoring_failure(&e);
            return None;
        }
    };
    Some(apply_contract(authored.criteria))
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::Value;

    use super::*;

    fn entry(kind: &str, content: &str) -> AskLedgerEntryRow {
        AskLedgerEntryRow {
            id: 1,
            session_id: 1,
            operator_turn_id: None,
            kind: kind.to_owned(),
            content: content.to_owned(),
            created_at: Utc::now(),
        }
    }

    fn authored(criterion: &str, provenance: &str, span: Option<&str>) -> AuthoredCriterion {
        AuthoredCriterion {
            criterion: criterion.to_owned(),
            provenance: provenance.to_owned(),
            operator_span: span.map(str::to_owned),
        }
    }

    #[test]
    fn the_sent_message_contains_only_operator_text() {
        let ledger = vec![
            entry(
                "initial",
                "make the comms panel work properly on the session page",
            ),
            entry("clarification", "it should not need a direct URL"),
        ];
        let sent = build_user_message(
            &ledger,
            0,
            "projects/temet/temet-proxy",
            IntakeInputBudget::V1,
        );

        assert!(sent.contains("make the comms panel work properly"));
        assert!(sent.contains("it should not need a direct URL"));
        assert!(sent.contains("projects/temet/temet-proxy"));

        for session_authored in [
            "I'll add a CommsPanel component",
            "Plan: 1. create the component 2. wire it up",
            "TODO: add comms panel",
            "Here's what I understand the task to be",
        ] {
            assert!(
                !sent.contains(session_authored),
                "the worker's message must not carry the session's own restatement: {session_authored:?}"
            );
        }
    }

    #[test]
    fn the_ledger_is_sent_oldest_first_with_its_kinds() {
        let ledger = vec![
            entry("initial", "first thing"),
            entry("correction", "actually, the other thing"),
        ];
        let sent = build_user_message(&ledger, 0, "p", IntakeInputBudget::V1);
        let first = sent.find("first thing").unwrap_or(usize::MAX);
        let second = sent.find("the other thing").unwrap_or(0);
        assert!(first < second, "oldest first");
        assert!(sent.contains("(initial)") && sent.contains("(correction)"));
    }

    #[test]
    fn artifact_scoped_criteria_never_reach_the_map() {
        let kept = apply_contract(vec![
            authored(
                "Add a comms panel to the session page.",
                "operator",
                Some("add a comms panel"),
            ),
            authored(
                "The comms panel is reachable from the session page without a direct URL.",
                "operator",
                Some("it should not need a direct URL"),
            ),
            authored("Update the component", "inferred", None),
        ]);
        assert_eq!(kept.len(), 1, "only the surface-scoped criterion survives");
        assert!(
            kept[0]
                .criterion
                .starts_with("The comms panel is reachable")
        );
    }

    #[test]
    fn an_unquoted_operator_claim_is_demoted_to_inferred() {
        let kept = apply_contract(vec![
            authored(
                "The panel is reachable without a direct URL.",
                "operator",
                None,
            ),
            authored(
                "The panel is reachable without a direct URL.",
                "operator",
                Some("   "),
            ),
        ]);
        assert_eq!(kept.len(), 2);
        for c in &kept {
            assert_eq!(c.provenance, "inferred");
            assert_eq!(c.operator_span, None);
        }
    }

    #[test]
    fn an_unrecognised_provenance_is_dropped_rather_than_guessed() {
        let kept = apply_contract(vec![authored(
            "The panel is reachable without a direct URL.",
            "assumed",
            Some("something"),
        )]);
        assert!(kept.is_empty());
    }

    #[test]
    fn an_operator_criterion_keeps_the_words_it_rests_on() {
        let kept = apply_contract(vec![authored(
            "The panel is reachable without a direct URL.",
            "operator",
            Some("  it should not need a direct URL  "),
        )]);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].provenance, "operator");
        assert_eq!(
            kept[0].operator_span.as_deref(),
            Some("it should not need a direct URL"),
            "trimmed, but the operator's words"
        );
    }

    fn seen(count: i32, newest: Option<i64>) -> Option<AskSeen> {
        Some(AskSeen {
            ask_entry_count: count,
            newest_ask_entry_id: newest,
        })
    }

    #[test]
    fn a_reauthor_is_only_paid_for_when_the_ask_has_moved() {
        assert!(!should_reauthor(None, None), "no ask, no call");
        assert!(should_reauthor(None, Some(1)), "first ask authors");
        assert!(
            !should_reauthor(seen(2, Some(9)), Some(9)),
            "unchanged ledger does not re-author"
        );
        assert!(
            should_reauthor(seen(2, Some(9)), Some(10)),
            "a new operator turn does"
        );
        assert!(
            !should_reauthor(seen(3, Some(9)), Some(7)),
            "a ledger whose newest ask is older than the one seen is not a reason to spend"
        );
        assert!(
            !should_reauthor(seen(2, Some(9)), None),
            "an empty ledger never authors"
        );
    }

    #[test]
    fn a_session_past_the_fetch_cap_still_reauthors_when_a_new_ask_lands() {
        let at_cap = 40;
        assert!(
            should_reauthor(seen(at_cap, Some(500)), Some(501)),
            "the stored count sits at the fetch cap, the newest id still moves"
        );
        assert!(!should_reauthor(seen(at_cap, Some(500)), Some(500)));
    }

    #[test]
    fn a_map_with_no_recorded_newest_ask_authors_once_the_ledger_has_one() {
        assert!(should_reauthor(seen(5, None), Some(3)));
        assert!(!should_reauthor(seen(5, None), None));
    }

    #[test]
    fn the_sent_message_says_how_many_earlier_asks_it_leaves_out() {
        let ledger = vec![entry("initial", "newest ask")];
        let sent = build_user_message(&ledger, 12, "p", IntakeInputBudget::V1);
        assert!(sent.contains("(12 earlier asks not shown)"));
        assert!(
            sent.contains("[13] (initial)"),
            "numbering continues the ledger"
        );
        let whole = build_user_message(&ledger, 0, "p", IntakeInputBudget::V1);
        assert!(!whole.contains("earlier asks not shown"));
    }

    #[test]
    fn an_oversized_project_path_is_cut_with_its_remainder_announced() {
        let sent = build_user_message(
            &[entry("initial", "ask")],
            0,
            &"p".repeat(budget::PROJECT_PATH_CHARS + 30),
            IntakeInputBudget::V1,
        );
        assert!(sent.contains("[... 30 more characters]"));
    }

    #[test]
    fn the_uncapped_message_sends_an_over_budget_ledger_and_path_uncut() {
        let big = "q".repeat(budget::ASK_CHARS + 700);
        let ledger: Vec<AskLedgerEntryRow> =
            (0..10).map(|_| entry("clarification", &big)).collect();
        let path = "p".repeat(budget::PROJECT_PATH_CHARS + 30);
        let capped = build_user_message(&ledger, 0, &path, IntakeInputBudget::V1);
        let whole = build_user_message(&ledger, 0, &path, IntakeInputBudget::Uncapped);
        assert!(capped.contains("more characters]"));
        assert!(capped.contains("earlier asks not shown"));
        assert!(!whole.contains("more characters]"));
        assert!(!whole.contains("earlier asks not shown"));
        assert_eq!(whole.matches(big.as_str()).count(), 10);
        assert!(whole.contains(&path));
    }

    #[test]
    fn the_schema_requires_provenance_on_every_criterion() {
        let schema = authored_criteria_schema();
        let item = &schema["properties"]["criteria"]["items"];
        let strings = |v: &Value| -> Vec<String> {
            v.as_array()
                .map(|a| {
                    a.iter()
                        .map(|x| x.as_str().unwrap_or_default().to_owned())
                        .collect()
                })
                .unwrap_or_default()
        };
        let required = strings(&item["required"]);
        for field in ["provenance", "criterion", "operator_span"] {
            assert!(
                required.contains(&field.to_owned()),
                "{field} must be required"
            );
        }
        assert_eq!(
            strings(&item["properties"]["provenance"]["enum"]),
            vec!["operator".to_owned(), "inferred".to_owned()]
        );
    }

    #[test]
    fn authored_criteria_schema_is_bare_not_self_wrapped() {
        let schema = authored_criteria_schema();
        assert_eq!(
            schema["type"], "object",
            "a bare JSON schema has `type` at the top level"
        );
        assert!(
            schema.get("name").is_none(),
            "a `name` key here means this schema pre-wraps itself — attempt_verdict already \
             adds its own `name: \"steerer_verdict\"` wrapper, so a self-wrapped return here \
             double-nests the schema the model actually receives"
        );
        assert!(
            schema.get("strict").is_none(),
            "a top-level `strict` key here is the same self-wrap symptom as `name` above"
        );
        assert!(
            schema.get("schema").is_none(),
            "a nested `schema` key here means the real schema is one level too deep for \
             attempt_verdict's own wrapping to find it"
        );
    }
}
