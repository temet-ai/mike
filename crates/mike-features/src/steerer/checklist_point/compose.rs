use super::questions::{BROKEN, DONE, MISSING, NOT_SHOWN_BROKEN, OPEN_STATED, key};
use super::state::Criterion;
use crate::llm::decisions::point::Asked;
use crate::steerer::verdict::{ChecklistItem, Need, SteererVerdict};

pub const NOT_DONE_AT: f64 = 0.6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Done,
    OpenStated,
    Missing,
    Broken,
    NotShownBroken,
}

impl Status {
    pub fn parse(choice: &str, rule: bool) -> Option<Self> {
        match (rule, choice) {
            (false, DONE) => Some(Self::Done),
            (false, OPEN_STATED) => Some(Self::OpenStated),
            (false, MISSING) => Some(Self::Missing),
            (true, BROKEN) => Some(Self::Broken),
            (true, NOT_SHOWN_BROKEN) => Some(Self::NotShownBroken),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Done => DONE,
            Self::OpenStated => OPEN_STATED,
            Self::Missing => MISSING,
            Self::Broken => BROKEN,
            Self::NotShownBroken => NOT_SHOWN_BROKEN,
        }
    }

    fn faulted(self) -> bool {
        matches!(self, Self::Missing | Self::Broken)
    }

    fn verdict_status(self) -> &'static str {
        match self {
            Self::Done | Self::NotShownBroken => "done",
            Self::OpenStated => "partial",
            Self::Missing | Self::Broken => "not_done",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub criterion: String,
    pub inferred: bool,
    pub rule: bool,
    pub status: Status,
    pub probability: f64,
}

impl Item {
    pub fn flags(&self) -> bool {
        !self.inferred && self.status.faulted() && self.probability >= NOT_DONE_AT
    }

    fn all_clear_confidence(&self) -> f64 {
        if self.status.faulted() {
            1.0 - self.probability
        } else {
            self.probability
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Checklist {
    pub items: Vec<Item>,
}

impl Checklist {
    pub fn flagged(&self) -> Vec<&Item> {
        self.items.iter().filter(|i| i.flags()).collect()
    }

    pub fn open_stated(&self) -> Vec<&Item> {
        self.items
            .iter()
            .filter(|i| !i.inferred && i.status == Status::OpenStated)
            .collect()
    }

    pub fn fires(&self) -> bool {
        self.items.iter().any(Item::flags)
    }

    pub fn confidence(&self) -> f64 {
        let flagged = self.flagged();
        if !flagged.is_empty() {
            return flagged.iter().map(|i| i.probability).fold(0.0, f64::max);
        }
        let stated: Vec<&Item> = self.items.iter().filter(|i| !i.inferred).collect();
        if stated.is_empty() {
            return 0.0;
        }
        stated.iter().map(|i| i.all_clear_confidence()).sum::<f64>() / stated.len() as f64
    }

    pub fn reason(&self) -> String {
        let flagged = self.flagged();
        if flagged.is_empty() {
            return format!(
                "No stated criterion is shown missing or broken ({} checked, {} stated as still open).",
                self.items.len(),
                self.open_stated().len()
            );
        }
        let named: Vec<String> = flagged
            .iter()
            .map(|i| format!("\"{}\" ({})", i.criterion, i.status.as_str()))
            .collect();
        format!(
            "{} of {} criteria are missing or broken: {}.",
            flagged.len(),
            self.items.len(),
            named.join("; ")
        )
    }

    pub fn fallback_nudge(&self) -> String {
        match self.flagged().first() {
            Some(item) if item.rule => format!(
                "You set the rule \"{}\" and this turn looks like it breaks it. Check it and say whether it still holds.",
                item.criterion
            ),
            Some(item) => format!(
                "You asked for \"{}\" and I do not see that in this turn. If it is done, say where; if not, it is still open.",
                item.criterion
            ),
            None => "You asked for the remaining criteria and I do not see them in this turn. If they are done, say where; if not, they are still open.".to_owned(),
        }
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
                    status: i.status.verdict_status().to_owned(),
                    evidence: format!("jev checklist, {} {:.2}", i.status.as_str(), i.probability),
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
            let rule = c.is_rule();
            let (choice, probability) = asked.choice(&key(i))?;
            Some(Item {
                criterion: c.text.clone(),
                inferred: c.inferred,
                rule,
                status: Status::parse(choice, rule)?,
                probability,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Checklist { items })
}
