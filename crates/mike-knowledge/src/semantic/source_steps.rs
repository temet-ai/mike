use std::time::{Duration, Instant};

use crate::change_feed::{ChangeFeed, Generation};
use crate::store::models::RowChangedTable;

pub const SOURCE_BACKSTOP: Duration = Duration::from_secs(3_600);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceStep {
    Directives,
    Anchors,
    Corrections,
}

impl SourceStep {
    fn watched(self) -> &'static [RowChangedTable] {
        match self {
            Self::Directives => &[RowChangedTable::DossierDirectives],
            Self::Anchors => &[RowChangedTable::SemanticAnchors],
            Self::Corrections => &[RowChangedTable::CorrectionQualifications],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceTicket(Option<Generation>);

#[derive(Debug, Clone, PartialEq, Eq)]
struct EmptyMark {
    embedder_key: String,
    at: Generation,
    until: Instant,
}

#[derive(Debug, Default)]
pub struct SourceSteps {
    directives: Option<EmptyMark>,
    anchors: Option<EmptyMark>,
    corrections: Option<EmptyMark>,
}

impl SourceSteps {
    fn slot(&mut self, step: SourceStep) -> &mut Option<EmptyMark> {
        match step {
            SourceStep::Directives => &mut self.directives,
            SourceStep::Anchors => &mut self.anchors,
            SourceStep::Corrections => &mut self.corrections,
        }
    }

    pub fn forget(&mut self, step: SourceStep) {
        *self.slot(step) = None;
    }

    pub fn ticket(feed: &ChangeFeed) -> SourceTicket {
        SourceTicket(feed.is_fed().then(|| feed.generation()))
    }

    pub fn nothing_new(
        &mut self,
        step: SourceStep,
        feed: &ChangeFeed,
        embedder_key: &str,
        now: Instant,
    ) -> bool {
        feed.is_fed()
            && self.slot(step).as_ref().is_some_and(|mark| {
                mark.embedder_key == embedder_key
                    && now < mark.until
                    && !feed.changed_since(step.watched(), mark.at)
            })
    }

    pub fn settle(
        &mut self,
        step: SourceStep,
        ticket: SourceTicket,
        embedder_key: &str,
        found: usize,
        now: Instant,
    ) {
        *self.slot(step) = match (ticket.0, found) {
            (Some(at), 0) => Some(EmptyMark {
                embedder_key: embedder_key.to_owned(),
                at,
                until: now + SOURCE_BACKSTOP,
            }),
            _ => None,
        };
    }
}

#[cfg(test)]
#[path = "tests/source_steps_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/source_notify_tests.rs"]
mod notify_tests;

#[cfg(test)]
#[path = "tests/correction_notify_tests.rs"]
mod correction_notify_tests;
