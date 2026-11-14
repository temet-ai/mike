use super::questions::key;
use super::state::Criterion;
use crate::llm::decisions::point::Asked;
use crate::steerer::verdict::{ChecklistItem, Need, SteererVerdict};

pub const NOT_DONE_AT: f64 = 0.6;

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub criterion: String,
    pub inferred: bool,
    pub done_probability: f64,
}

impl Item {
    pub fn open(&self) -> bool {
        1.0 - self.done_probability >= NOT_DONE_AT
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Checklist {
    pub items: Vec<Item>,
}

impl Checklist {
    pub fn open_stated(&self) -> Vec<&Item> {
        self.items
            .iter()
            .filter(|i| i.open() && !i.inferred)
            .collect()
    }

    pub fn fires(&self) -> bool {
        !self.open_stated().is_empty()
    }

    pub fn confidence(&self) -> f64 {
        let open = self.open_stated();
        if open.is_empty() {
            let stated: Vec<&Item> = self.items.iter().filter(|i| !i.inferred).collect();
            if stated.is_empty() {
                return 0.0;
            }
            return stated.iter().map(|i| i.done_probability).sum::<f64>() / stated.len() as f64;
        }
        open.iter()
            .map(|i| 1.0 - i.done_probability)
            .fold(0.0, f64::max)
    }

    pub fn reason(&self) -> String {
        let open = self.open_stated();
        if open.is_empty() {
            return format!(
                "The checklist shows every stated criterion done ({} checked).",
                self.items.len()
            );
        }
        let named: Vec<String> = open
            .iter()
            .map(|i| format!("\"{}\"", i.criterion))
            .collect();
        format!(
            "{} of {} criteria are not shown done: {}.",
            open.len(),
            self.items.len(),
            named.join("; ")
        )
    }

    pub fn fallback_nudge(&self) -> String {
        let first = self
            .open_stated()
            .first()
            .map_or("the remaining criteria", |i| i.criterion.as_str());
        format!(
            "You asked for \"{first}\" and I do not see that in this turn. If it is done, say where; if not, it is still open."
        )
    }

    pub fn verdict(&self) -> SteererVerdict {
        let fires = self.fires();
        SteererVerdict {
            fires,
            confidence: self.confidence(),
            reason: self.reason(),
            nudge: if fires {
                self.fallback_nudge()
            } else {
                String::new()
            },
            need: if fires { Need::High } else { Need::Low },
            checklist: self
                .items
                .iter()
                .map(|i| ChecklistItem {
                    criterion: i.criterion.clone(),
                    status: if i.open() { "not_done" } else { "done" }.to_owned(),
                    evidence: format!("jev checklist, done {:.2}", i.done_probability),
                })
                .collect(),
        }
    }

    pub fn merged(&self, written: &SteererVerdict) -> SteererVerdict {
        let mut verdict = self.verdict();
        if !written.nudge.trim().is_empty() {
            verdict.nudge = written.nudge.clone();
        }
        verdict
    }
}

pub fn compose(asked: &Asked, criteria: &[Criterion]) -> Option<Checklist> {
    if asked.failed() || criteria.is_empty() {
        return None;
    }
    let items = criteria
        .iter()
        .enumerate()
        .map(|(i, c)| {
            Some(Item {
                criterion: c.text.clone(),
                inferred: c.inferred,
                done_probability: asked.noul(&key(i))?,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Checklist { items })
}
