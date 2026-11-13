use serde::Deserialize;
use serde_json::{Value, json};

use super::budget::{IntakeInputBudget, PROJECT_PATH_CHARS, fit_asks};
use crate::dossier::render::cut::cut;

#[derive(Debug, Deserialize)]
pub struct AuthoredCriteria {
    pub criteria: Vec<AuthoredCriterion>,
}

#[derive(Debug, Deserialize)]
pub struct AuthoredCriterion {
    pub criterion: String,
    pub provenance: String,
    #[serde(default)]
    pub operator_span: Option<String>,
}

pub const SYSTEM_PROMPT: &str = "\
You write acceptance criteria for a software task, from the operator's own words.

THE RULE: a criterion names what is TRUE OF THE SURFACE once the work is done. Never what to do.

A criterion whose subject is the artifact alone is not a criterion. \"Add X\", \"update Y\", \
\"create Z\", \"read the style guide and apply the change\" are instructions. They pass work that \
is plainly wrong, because the thing being added is not the deliverable — the surface it lands in is.

Worked example. The operator asked for a comms panel and rejected what was delivered, because it \
used old components, was not visible to the user, silently fell to paths that never fired, and had \
no place on the page. Every one of those satisfies \"add a comms panel\".

  BAD:  Add a comms panel to the session page.
  GOOD: The comms panel is reachable from the session page without a direct URL, is built on the \
current shared primitives, renders in place alongside its neighbours without disturbing them, and \
every branch it introduces is exercised by a real session.

Ask of each criterion: can this be answered by pointing at the surface? If answering it needs the \
diff rather than the result, it is about the artifact — rewrite it.

Four questions worth naming, because they are where work actually fails:
  1. Is it reachable? Something nothing routes to is created but not connected.
  2. Is it built on the current primitives, not superseded ones?
  3. Does it look like it belongs, next to its neighbours?
  4. Does every branch it introduces actually fire?

PROVENANCE. Mark a criterion \"operator\" only when it restates something the operator actually \
said, and quote their words in operator_span. Mark it \"inferred\" when you supplied it from \
context, and leave operator_span empty. Do not inflate: an inferred criterion honestly marked is \
useful, and one dressed up as the operator's is worse than none.

When the list says earlier asks are not shown, write criteria only from the asks that are shown, and \
never guess what the earlier ones said.

Return between 1 and 8 criteria. Fewer, sharper criteria beat a long list. If the operator's text \
contains no work-shaped ask at all, return an empty list.";

pub fn authored_criteria_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["criteria"],
        "properties": {
            "criteria": {
                "type": "array",
                "maxItems": 8,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["criterion", "provenance", "operator_span"],
                    "properties": {
                        "criterion": {
                            "type": "string",
                            "description": "What is true of the surface once the work is done."
                        },
                        "provenance": {
                            "type": "string",
                            "enum": ["operator", "inferred"]
                        },
                        "operator_span": {
                            "type": "string",
                            "description": "The operator's own words this rests on; empty for inferred."
                        }
                    }
                }
            }
        }
    })
}

pub fn build_user_message(
    entries: &[crate::store::persistence::memory_ask_ledger::AskLedgerEntryRow],
    older_not_fetched: usize,
    project_path: &str,
    budget: IntakeInputBudget,
) -> String {
    let asks = fit_asks(entries, older_not_fetched, budget);
    let mut out = String::new();
    out.push_str("Project: ");
    if budget == IntakeInputBudget::Uncapped {
        out.push_str(project_path);
    } else {
        out.push_str(&cut(project_path, PROJECT_PATH_CHARS));
    }
    out.push_str(
        "\n\nThe operator's own words for this session, oldest first. Nothing else in this \
         message came from the session.\n",
    );
    if asks.omitted > 0 {
        out.push_str(&format!("\n({} earlier asks not shown)\n", asks.omitted));
    }
    for (idx, ask) in asks.shown.iter().enumerate() {
        out.push_str(&format!(
            "\n[{}] ({})\n{}\n",
            asks.omitted + idx + 1,
            ask.kind,
            ask.text
        ));
    }
    out.push_str(
        "\nWrite the acceptance criteria for this ask, as states of the surface after the work.",
    );
    out
}
