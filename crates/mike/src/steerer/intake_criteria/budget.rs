use crate::dossier::render::cut::{char_count, cut};
use crate::store::persistence::memory_ask_ledger::AskLedgerEntryRow;

pub(crate) const PROJECT_PATH_CHARS: usize = 400;
pub(crate) const ASK_CHARS: usize = 4_000;
pub(crate) const ASKS_SECTION_CHARS: usize = 24_000;
const ASK_FRAME_CHARS: usize = 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IntakeInputBudget {
    Uncapped,
    V1,
}

impl IntakeInputBudget {
    pub(crate) fn from_setting(value: &str) -> Self {
        match value {
            "uncapped" => Self::Uncapped,
            "intake-criteria-input-v1" => Self::V1,
            other => {
                tracing::warn!(
                    value = other,
                    "steerer.intake_criteria.input_budget holds an unknown code; using intake-criteria-input-v1"
                );
                Self::V1
            }
        }
    }
}

pub(crate) struct ShownAsk<'a> {
    pub(crate) kind: &'a str,
    pub(crate) text: String,
}

pub(crate) struct AsksSection<'a> {
    pub(crate) shown: Vec<ShownAsk<'a>>,
    pub(crate) omitted: usize,
}

pub(crate) fn fit_asks(
    entries: &[AskLedgerEntryRow],
    older_not_fetched: usize,
    budget: IntakeInputBudget,
) -> AsksSection<'_> {
    if budget == IntakeInputBudget::Uncapped {
        return AsksSection {
            shown: entries
                .iter()
                .map(|entry| ShownAsk {
                    kind: &entry.kind,
                    text: entry.content.trim().to_owned(),
                })
                .collect(),
            omitted: older_not_fetched,
        };
    }
    let mut used = 0usize;
    let mut shown: Vec<ShownAsk<'_>> = Vec::new();
    for entry in entries.iter().rev() {
        let text = cut(entry.content.trim(), ASK_CHARS);
        let cost = char_count(&text) + ASK_FRAME_CHARS;
        if !shown.is_empty() && used + cost > ASKS_SECTION_CHARS {
            break;
        }
        used += cost;
        shown.push(ShownAsk {
            kind: &entry.kind,
            text,
        });
    }
    shown.reverse();
    AsksSection {
        omitted: older_not_fetched + entries.len() - shown.len(),
        shown,
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    fn entry(id: i64, content: &str) -> AskLedgerEntryRow {
        AskLedgerEntryRow {
            id,
            session_id: 1,
            operator_turn_id: None,
            kind: "clarification".to_owned(),
            content: content.to_owned(),
            created_at: Utc::now(),
        }
    }

    #[test]
    fn a_ledger_inside_the_budget_is_shown_whole() {
        let entries = vec![entry(1, "first"), entry(2, "second")];
        let section = fit_asks(&entries, 0, IntakeInputBudget::V1);
        assert_eq!(section.omitted, 0);
        assert_eq!(section.shown.len(), 2);
        assert_eq!(section.shown[0].text, "first");
        assert_eq!(section.shown[1].text, "second");
    }

    #[test]
    fn an_over_budget_ledger_keeps_the_newest_asks_and_counts_the_rest() {
        let body = "x".repeat(ASK_CHARS);
        let entries: Vec<AskLedgerEntryRow> = (1..=20).map(|id| entry(id, &body)).collect();
        let section = fit_asks(&entries, 0, IntakeInputBudget::V1);
        let fits = ASKS_SECTION_CHARS / (ASK_CHARS + ASK_FRAME_CHARS);
        assert_eq!(section.shown.len(), fits);
        assert_eq!(section.omitted, 20 - fits);
        assert!(section.shown.len() < entries.len());
    }

    #[test]
    fn asks_not_fetched_are_counted_with_the_ones_dropped_here() {
        let body = "y".repeat(ASK_CHARS);
        let entries: Vec<AskLedgerEntryRow> = (1..=10).map(|id| entry(id, &body)).collect();
        let section = fit_asks(&entries, 7, IntakeInputBudget::V1);
        assert_eq!(section.omitted, 7 + entries.len() - section.shown.len());
    }

    #[test]
    fn a_single_oversized_ask_is_cut_with_its_remainder_announced() {
        let body = "z".repeat(ASK_CHARS + 500);
        let only = [entry(1, &body)];
        let section = fit_asks(&only, 0, IntakeInputBudget::V1);
        assert_eq!(section.shown.len(), 1);
        assert_eq!(section.omitted, 0);
        assert!(section.shown[0].text.ends_with("[... 500 more characters]"));
    }

    #[test]
    fn the_newest_ask_is_the_last_one_shown() {
        let entries = vec![entry(1, "old"), entry(2, &"n".repeat(ASK_CHARS * 3))];
        let section = fit_asks(&entries, 0, IntakeInputBudget::V1);
        assert!(!section.shown.is_empty());
        assert!(
            section
                .shown
                .last()
                .is_some_and(|s| s.text.starts_with('n'))
        );
    }

    #[test]
    fn uncapped_shows_an_over_budget_ledger_whole_and_uncut() {
        let body = "w".repeat(ASK_CHARS + 500);
        let entries: Vec<AskLedgerEntryRow> = (1..=20).map(|id| entry(id, &body)).collect();
        let section = fit_asks(&entries, 3, IntakeInputBudget::Uncapped);
        assert_eq!(section.shown.len(), 20);
        assert_eq!(section.omitted, 3);
        assert!(section.shown.iter().all(|s| s.text == body));
        assert!(!section.shown[0].text.contains("more characters"));
    }

    #[test]
    fn setting_codes_map_to_budgets_and_unknown_falls_back_to_the_capped_one() {
        assert_eq!(
            IntakeInputBudget::from_setting("uncapped"),
            IntakeInputBudget::Uncapped
        );
        assert_eq!(
            IntakeInputBudget::from_setting("intake-criteria-input-v1"),
            IntakeInputBudget::V1
        );
        assert_eq!(
            IntakeInputBudget::from_setting("nonsense"),
            IntakeInputBudget::V1
        );
    }

    #[test]
    fn an_empty_ledger_has_nothing_to_show_or_omit() {
        let section = fit_asks(&[], 0, IntakeInputBudget::V1);
        assert!(section.shown.is_empty());
        assert_eq!(section.omitted, 0);
    }
}
