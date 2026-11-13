#[cfg(test)]
pub mod tests {
    use chrono::Utc;
    use serde::Deserialize;
    use serde_json::{Value, json};

    use crate::llm::OpenRouterClient;
    use crate::steerer::intake_criteria;
    use crate::store::persistence::acceptance_criteria::NewCriterion;
    use crate::store::persistence::memory_ask_ledger::AskLedgerEntryRow;

    fn entry(kind: &str, content: &str) -> AskLedgerEntryRow {
        AskLedgerEntryRow {
            id: 0,
            session_id: 0,
            operator_turn_id: None,
            kind: kind.to_owned(),
            content: content.to_owned(),
            created_at: Utc::now(),
        }
    }

    fn original_comms_ask() -> Vec<AskLedgerEntryRow> {
        vec![
            entry(
                "initial",
                "No. im talking about the mailbox dedicated pages that can show us the \
                 conversations and messages from sessions to each other. with enhancements to \
                 have proper threads, agents, messages from to replies",
            ),
            entry(
                "clarification",
                "I want you to actually plan and implement it for me create a plan to do the \
                 work and do it now",
            ),
            entry("correction", "its not wired into the nav"),
            entry(
                "clarification",
                "the view for threads are quite weird. a thread and it's message should be \
                 able to be talked about. turtles all the way. you should be able to add \
                 context, and items to the message. a message should visually try to stay \
                 perhaps like a slack thread message",
            ),
        ]
    }

    fn original_157_ask() -> Vec<AskLedgerEntryRow> {
        vec![entry(
            "initial",
            "Whole-surface verification for any change with a visual footprint: render the \
             full route in the running app, with the change in place and its real neighbours \
             around it. The evidence artifact is a page screenshot, not a component \
             screenshot — if the change touches a component used on several routes, every \
             affected route is rendered. Affected routes are derived from the import graph, \
             not from the session's memory of where the component is used.",
        )]
    }

    const COMMS_SHIPPED_OBSERVATIONS: &str = "\
Operator's own account of the delivered feature, 2026-08-16: \"You build the feature, but when \
I look at the implementation it uses old components, it isn't visible to the user, it silently \
falls to paths that never fire, it's not properly integrated into sessions, or have its own UI \
design and placement.\"

Independently machine-verified findings (steerer::structural, re-run live against the current \
tree, not asserted from memory):
- Every file under frontend/src/lib/comms/ imports zero components from the canonical \
  $lib/components/ui/ primitive set, while four of its files hand-roll 19 raw <button> \
  elements between them. A comparison surface, frontend/src/lib/components/shell/, imports \
  Dialog/Popover/Tooltip from that same canonical set extensively.
- Every comms component IS currently reachable from a route (/comms and \
  /sessions/[sessionId]/comms both link to it) — this was fixed by a later rebuild \
  (f1152504, 2026-08-17), after the operator's quote above.
- The context-item `kind` field has 9 declared values (a DB CHECK constraint and an identical \
  frontend TypeScript union). Across 714 real send_session_message calls this repository has \
  captured of its own traffic, the `note` kind has never once been used.";

    const AC157_DELIVERED_OBSERVATIONS: &str = "\
Independent verifier's recorded verdict (not the implementing session's self-report), run \
against a frozen commit cut specifically for verification, detached from the live worktree:
- The harness renders the FULL ROUTE in the running app, not an isolated component — its \
  rendering targets a real SvelteKit route served by the live inspect backend (not a \
  Storybook-style isolated component render), so each output image is a PAGE screenshot \
  showing the route with its real neighbouring page chrome around it, not a component cropped \
  out of context. The verifier's two captured renders were 1440x3553 PNGs — full-page \
  scrollable-height captures, consistent with a whole route rather than a cropped component \
  bounding box.
- AFFECTED ROUTES ARE DERIVED FROM THE IMPORT GRAPH, not declared by the session, and this was \
  proven live rather than assumed: the verifier created an untracked, never-committed route \
  (frontend/routes/__verifier_probe__/+page.svelte) that had not existed a minute earlier; it \
  appeared IMMEDIATELY in the affected-route set with a correct import chain, and vanished when \
  the file was deleted — ruling out a stale index, a cache, or a checked-in fixture, and \
  proving the tool reads the live import graph rather than consulting a fixed or remembered \
  list. The three-state demo independently confirmed the same property (baseline one route; \
  widen to include /tools via a real one-hop import the verifier wrote itself, not the \
  implementer's diff; revert to byte-identical).
- EVERY AFFECTED ROUTE IS RENDERED, and a route that cannot be is reported, never silently \
  dropped: the render manifest recorded a route missing its required --param as `rendered: \
  false` with a named reason, rather than omitting it from the output.
- The mutation test suite reproduced precisely: 9/9 passing unmutated, exactly 1/9 failing on \
  the intended mutation (isPureBarrel = false, which falsely flags all 39 routes), restored \
  clean afterward — confirming the discovery logic is doing real work, not vacuously passing.
- One real defect was found during the adversarial verification itself (a computed, \
  non-literal dynamic import specifier produced zero edges instead of falling back to the \
  documented safe-include behaviour) and fixed in flight by the implementing session, not \
  after being told about it.";

    fn live_client() -> Option<OpenRouterClient> {
        let key = crate::llm::key::seed_from_env(None)?;
        use secrecy::ExposeSecret;
        OpenRouterClient::new(
            key.expose_secret().to_owned(),
            "google/gemini-3.5-flash-lite".to_owned(),
            None,
            Some(std::time::Duration::from_secs(30)),
        )
        .ok()
    }

    fn grading_schema() -> Value {
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["overall_pass", "per_criterion"],
            "properties": {
                "overall_pass": {
                    "type": "boolean",
                    "description": "true only if every criterion is satisfied by the observations"
                },
                "per_criterion": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["index", "satisfied", "reason"],
                        "properties": {
                            "index": {"type": "integer"},
                            "satisfied": {"type": "boolean"},
                            "reason": {"type": "string"}
                        }
                    }
                }
            }
        })
    }

    #[derive(Debug, Deserialize)]
    struct CriteriaGrade {
        overall_pass: bool,
        per_criterion: Vec<PerCriterionGrade>,
    }

    #[derive(Debug, Deserialize)]
    struct PerCriterionGrade {
        index: u32,
        satisfied: bool,
        reason: String,
    }

    const GRADER_SYSTEM_PROMPT: &str = "\
You grade whether a set of numbered acceptance criteria is satisfied by a block of factual \
observations about a delivered surface. The observations are independently verified, not a \
self-report from whoever built the surface — treat them as ground truth. For each criterion, \
decide whether the observations show it is satisfied; do not assume a criterion passes just \
because the observations don't explicitly contradict it — an unaddressed criterion is not \
satisfied. overall_pass is true only when every criterion is satisfied.";

    fn build_grading_user_message(criteria: &[NewCriterion], observations: &str) -> String {
        let mut out = String::new();
        out.push_str("Criteria:\n");
        for (i, c) in criteria.iter().enumerate() {
            out.push_str(&format!("{}. {}\n", i, c.criterion));
        }
        out.push_str("\nObservations about the delivered surface:\n");
        out.push_str(observations);
        out
    }

    async fn grade(
        llm: &OpenRouterClient,
        criteria: &[NewCriterion],
        observations: &str,
    ) -> CriteriaGrade {
        let user = build_grading_user_message(criteria, observations);
        match llm
            .verdict(GRADER_SYSTEM_PROMPT, &user, grading_schema())
            .await
        {
            Ok(grade) => grade,
            Err(e) => panic!("grading call failed: {e}"),
        }
    }

    fn assert_grade_internally_consistent(grade: &CriteriaGrade) {
        let any_unsatisfied = grade.per_criterion.iter().any(|c| !c.satisfied);
        assert_eq!(
            grade.overall_pass, !any_unsatisfied,
            "overall_pass ({}) disagrees with the per-criterion verdicts: {:?}",
            grade.overall_pass, grade.per_criterion
        );
        for c in &grade.per_criterion {
            assert!(
                !c.reason.trim().is_empty(),
                "criterion {} has no reason — index {}, satisfied {}",
                c.index,
                c.index,
                c.satisfied
            );
        }
    }

    #[test]
    fn the_historical_asks_and_observations_are_the_real_ones() {
        let comms = original_comms_ask();
        assert_eq!(comms.len(), 4);
        assert!(comms[0].content.contains("mailbox dedicated pages"));
        assert!(comms[2].content.contains("not wired into the nav"));

        let ac157 = original_157_ask();
        assert_eq!(ac157.len(), 1);
        assert!(
            ac157[0]
                .content
                .contains("Affected routes are derived from the import graph")
        );

        assert!(COMMS_SHIPPED_OBSERVATIONS.contains("it uses old components"));
        assert!(COMMS_SHIPPED_OBSERVATIONS.contains("zero components from the canonical"));
        assert!(AC157_DELIVERED_OBSERVATIONS.contains("FULL ROUTE in the running app"));
        assert!(AC157_DELIVERED_OBSERVATIONS.contains("DERIVED FROM THE IMPORT GRAPH"));
    }

    #[ignore = "hits a live OpenRouter model call twice (author + grade) — run manually: \
                cargo test -p mike --lib steerer::ac2_demo -- --ignored --nocapture"]
    #[tokio::test]
    async fn criteria_from_the_comms_ask_fail_the_shipped_comms_feature() {
        let Some(llm) = live_client() else {
            eprintln!(
                "skipping: no OpenRouter key in the environment (see llm::key::seed_from_env)"
            );
            return;
        };

        let criteria = intake_criteria::author(
            &llm,
            &original_comms_ask(),
            0,
            "temet-proxy",
            intake_criteria::IntakeInputBudget::V1,
        )
        .await
        .unwrap_or_else(|| {
            panic!(
                "authoring produced no criteria — either the call failed closed, or the \
                     contract rejected every criterion the model wrote"
            )
        });
        assert!(
            !criteria.is_empty(),
            "authoring returned an empty criteria set"
        );

        let grade = grade(&llm, &criteria, COMMS_SHIPPED_OBSERVATIONS).await;
        assert_grade_internally_consistent(&grade);
        assert!(
            !grade.overall_pass,
            "expected the shipped comms feature to FAIL its own criteria (per_criterion: {:?}), \
             but the grader passed it — either the criteria are too weak, or the comms surface \
             has since been fully fixed (re-check steerer::structural's live findings before \
             assuming the former)",
            grade.per_criterion
        );
    }

    #[ignore = "hits a live OpenRouter model call twice (author + grade) — run manually: \
                cargo test -p mike --lib steerer::ac2_demo -- --ignored --nocapture"]
    #[tokio::test]
    async fn criteria_from_the_ac157_ask_pass_the_verified_ac157_delivery() {
        let Some(llm) = live_client() else {
            eprintln!(
                "skipping: no OpenRouter key in the environment (see llm::key::seed_from_env)"
            );
            return;
        };

        let criteria = intake_criteria::author(
            &llm,
            &original_157_ask(),
            0,
            "temet-proxy",
            intake_criteria::IntakeInputBudget::V1,
        )
        .await
        .unwrap_or_else(|| {
            panic!(
                "authoring produced no criteria — either the call failed closed, or the \
                     contract rejected every criterion the model wrote"
            )
        });
        assert!(
            !criteria.is_empty(),
            "authoring returned an empty criteria set"
        );

        let grade = grade(&llm, &criteria, AC157_DELIVERED_OBSERVATIONS).await;
        assert_grade_internally_consistent(&grade);
        assert!(
            grade.overall_pass,
            "expected #157's verified delivery to PASS its own criteria (per_criterion: {:?}), \
             but the grader failed it — either the criteria are too strict (a contract that \
             fails everything is not doing the job either, per acceptance-criteria-contract.md), \
             or #157's delivery genuinely regressed since the 12:26Z verdict",
            grade.per_criterion
        );
    }
}
