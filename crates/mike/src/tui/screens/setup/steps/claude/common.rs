use crate::finding::Finding;
use crate::tui::screens::setup::panel::PanelEvent;
use crate::tui::screens::setup::step::Outcome;

pub(super) fn base_outcome(event: PanelEvent) -> Option<Outcome> {
    match event {
        PanelEvent::Next | PanelEvent::Skip => Some(Outcome::Next),
        PanelEvent::Back => Some(Outcome::Back),
        PanelEvent::FinishLater => Some(Outcome::FinishLater),
        PanelEvent::Idle => Some(Outcome::Stay),
        PanelEvent::Run | PanelEvent::Recheck => None,
    }
}

pub(super) fn failure(error: impl std::fmt::Display) -> Vec<Finding> {
    vec![Finding::failed(error.to_string())]
}
