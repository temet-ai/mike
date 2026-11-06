use chrono::{DateTime, Duration, Utc};

use crate::settings::SettingsOverlay;
use crate::steerer::verdict::{Need, SteererVerdict, clears_confidence_threshold};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::steering::SessionNote;

const NOTES_READ: i64 = 40;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct NoteLimits {
    pub(crate) per_window: usize,
    pub(crate) window: Duration,
    pub(crate) wrong_note_mute: Duration,
}

impl NoteLimits {
    pub(crate) fn from_settings(settings: &SettingsOverlay) -> Self {
        Self {
            per_window: usize::try_from(settings.steerer_session_note_limit.max(1)).unwrap_or(1),
            window: Duration::minutes(settings.steerer_session_note_window_minutes.max(1)),
            wrong_note_mute: Duration::hours(settings.steerer_wrong_note_mute_hours.max(0)),
        }
    }

    fn look_back(&self) -> Duration {
        self.window.max(self.wrong_note_mute)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NoteRefusal {
    SessionNoteLimit,
    QuietAfterWrongNote,
    NeedBelowLevel,
}

impl NoteRefusal {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::SessionNoteLimit => "session_note_limit",
            Self::QuietAfterWrongNote => "quiet_after_wrong_note",
            Self::NeedBelowLevel => "need_below_level",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VerdictStop {
    Declined,
    ConfidenceBelowThreshold,
    Note(NoteRefusal),
}

#[derive(Debug, Clone)]
pub(crate) struct SessionNotes {
    notes: Vec<SessionNote>,
    at: DateTime<Utc>,
    limits: NoteLimits,
}

impl SessionNotes {
    pub(crate) fn new(notes: Vec<SessionNote>, at: DateTime<Utc>, limits: NoteLimits) -> Self {
        Self { notes, at, limits }
    }

    pub(crate) async fn load(
        persist: &PersistHandle,
        session_db_id: i64,
        limits: NoteLimits,
        at: DateTime<Utc>,
    ) -> Self {
        let notes = match persist
            .session_notes_since(session_db_id, at - limits.look_back(), at, NOTES_READ)
            .await
        {
            Ok(notes) => notes,
            Err(error) => {
                tracing::warn!(session_db_id, %error, "steering: could not read this session's earlier notes; judging as if there were none");
                Vec::new()
            }
        };
        Self::new(notes, at, limits)
    }

    fn count_in_window(&self) -> usize {
        let since = self.at - self.limits.window;
        self.notes
            .iter()
            .filter(|note| note.created_at >= since)
            .count()
    }

    fn muted(&self, steerer_id: i64) -> bool {
        let since = self.at - self.limits.wrong_note_mute;
        self.notes
            .iter()
            .find(|note| note.steerer_id == steerer_id)
            .is_some_and(|last| {
                last.created_at >= since && last.outcome_label.as_deref() == Some("wrong")
            })
    }

    pub(crate) fn need_required(&self) -> Need {
        if self.count_in_window() > 0 {
            Need::High
        } else {
            Need::Medium
        }
    }

    fn before_judging(&self, steerer_id: i64) -> Option<NoteRefusal> {
        if self.count_in_window() >= self.limits.per_window {
            return Some(NoteRefusal::SessionNoteLimit);
        }
        self.muted(steerer_id)
            .then_some(NoteRefusal::QuietAfterWrongNote)
    }
}

pub(crate) fn before_judging(notes: Option<&SessionNotes>, steerer_id: i64) -> Option<NoteRefusal> {
    notes?.before_judging(steerer_id)
}

pub(crate) fn after_verdict(notes: Option<&SessionNotes>, need: Need) -> Option<NoteRefusal> {
    let required = notes.map_or(Need::Medium, SessionNotes::need_required);
    (need < required).then_some(NoteRefusal::NeedBelowLevel)
}

pub(crate) fn refusal(
    notes: Option<&SessionNotes>,
    steerer_id: i64,
    need: Need,
) -> Option<NoteRefusal> {
    before_judging(notes, steerer_id).or_else(|| after_verdict(notes, need))
}

pub(crate) fn stop_before_quorum(
    verdict: &SteererVerdict,
    confidence_threshold: f64,
    notes: Option<&SessionNotes>,
) -> Option<VerdictStop> {
    if !verdict.fires {
        return Some(VerdictStop::Declined);
    }
    if !clears_confidence_threshold(verdict, confidence_threshold) {
        return Some(VerdictStop::ConfidenceBelowThreshold);
    }
    after_verdict(notes, verdict.need).map(VerdictStop::Note)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> NoteLimits {
        NoteLimits {
            per_window: 3,
            window: Duration::minutes(60),
            wrong_note_mute: Duration::hours(24),
        }
    }

    fn at() -> DateTime<Utc> {
        "2026-10-05T12:00:00Z".parse().unwrap_or_default()
    }

    fn note(steerer_id: i64, minutes_ago: i64, label: Option<&str>) -> SessionNote {
        SessionNote {
            steerer_id,
            steerer_name: format!("steerer-{steerer_id}"),
            payload_text: Some(format!("note from {minutes_ago} minutes ago")),
            created_at: at() - Duration::minutes(minutes_ago),
            outcome_label: label.map(str::to_owned),
            reaction: label.map(|_| "declined".to_owned()),
        }
    }

    fn notes(list: Vec<SessionNote>) -> SessionNotes {
        SessionNotes::new(list, at(), limits())
    }

    fn verdict(fires: bool, confidence: f64, need: Need) -> SteererVerdict {
        SteererVerdict {
            fires,
            confidence,
            reason: String::new(),
            nudge: "check the export".to_owned(),
            need,
            checklist: Vec::new(),
        }
    }

    #[test]
    fn the_first_note_to_a_session_needs_medium_and_later_ones_need_high() {
        let quiet = notes(vec![note(6, 90, None)]);
        assert_eq!(
            after_verdict(Some(&quiet), Need::Low),
            Some(NoteRefusal::NeedBelowLevel)
        );
        assert_eq!(after_verdict(Some(&quiet), Need::Medium), None);
        let after_one = notes(vec![note(6, 10, None)]);
        assert_eq!(
            after_verdict(Some(&after_one), Need::Medium),
            Some(NoteRefusal::NeedBelowLevel)
        );
        assert_eq!(after_verdict(Some(&after_one), Need::High), None);
    }

    #[test]
    fn without_a_history_the_first_note_rule_applies() {
        assert_eq!(after_verdict(None, Need::Medium), None);
        assert_eq!(
            after_verdict(None, Need::Low),
            Some(NoteRefusal::NeedBelowLevel)
        );
        assert_eq!(before_judging(None, 6), None);
    }

    #[test]
    fn a_session_at_its_limit_is_not_judged_until_the_window_moves_on() {
        let three = notes(vec![
            note(6, 5, None),
            note(15, 20, None),
            note(7, 59, None),
        ]);
        assert_eq!(
            before_judging(Some(&three), 99),
            Some(NoteRefusal::SessionNoteLimit)
        );
        let two_and_an_old_one = notes(vec![
            note(6, 5, None),
            note(15, 20, None),
            note(7, 61, None),
        ]);
        assert_eq!(before_judging(Some(&two_and_an_old_one), 99), None);
    }

    #[test]
    fn a_steerer_is_quiet_for_a_session_after_its_last_note_there_was_wrong() {
        let after_wrong = notes(vec![
            note(6, 300, Some("wrong")),
            note(15, 400, Some("right")),
        ]);
        assert_eq!(
            before_judging(Some(&after_wrong), 6),
            Some(NoteRefusal::QuietAfterWrongNote)
        );
        assert_eq!(before_judging(Some(&after_wrong), 15), None);
        let wrong_then_right = notes(vec![
            note(6, 30, Some("right")),
            note(6, 300, Some("wrong")),
        ]);
        assert_eq!(before_judging(Some(&wrong_then_right), 6), None);
        let long_ago = notes(vec![note(6, 25 * 60, Some("wrong"))]);
        assert_eq!(before_judging(Some(&long_ago), 6), None);
    }

    #[test]
    fn one_question_covers_a_producer_that_judges_before_it_asks() {
        let after_one = notes(vec![note(6, 10, None)]);
        assert_eq!(
            refusal(Some(&after_one), 21, Need::Medium),
            Some(NoteRefusal::NeedBelowLevel)
        );
        assert_eq!(refusal(Some(&after_one), 21, Need::High), None);
        let three = notes(vec![
            note(6, 5, None),
            note(15, 20, None),
            note(7, 59, None),
        ]);
        assert_eq!(
            refusal(Some(&three), 21, Need::High),
            Some(NoteRefusal::SessionNoteLimit)
        );
    }

    #[test]
    fn a_verdict_is_stopped_by_the_first_check_it_fails() {
        let after_one = notes(vec![note(6, 10, None)]);
        assert_eq!(
            stop_before_quorum(&verdict(false, 0.99, Need::High), 0.7, None),
            Some(VerdictStop::Declined)
        );
        assert_eq!(
            stop_before_quorum(&verdict(true, 0.5, Need::High), 0.7, None),
            Some(VerdictStop::ConfidenceBelowThreshold)
        );
        assert_eq!(
            stop_before_quorum(&verdict(true, 0.9, Need::Medium), 0.7, Some(&after_one)),
            Some(VerdictStop::Note(NoteRefusal::NeedBelowLevel))
        );
        assert_eq!(
            stop_before_quorum(&verdict(true, 0.9, Need::High), 0.7, Some(&after_one)),
            None
        );
    }

    #[test]
    fn limits_come_from_settings_and_never_fall_below_one_note() {
        let mut settings = SettingsOverlay::default();
        assert_eq!(NoteLimits::from_settings(&settings), limits());
        settings.steerer_session_note_limit = 0;
        settings.steerer_wrong_note_mute_hours = 0;
        let tight = NoteLimits::from_settings(&settings);
        assert_eq!(tight.per_window, 1);
        assert_eq!(tight.wrong_note_mute, Duration::zero());
    }

    #[test]
    fn every_refusal_has_one_code_used_by_all_steerers() {
        for refusal in [
            NoteRefusal::SessionNoteLimit,
            NoteRefusal::QuietAfterWrongNote,
            NoteRefusal::NeedBelowLevel,
        ] {
            assert_eq!(
                serde_json::json!(refusal),
                serde_json::json!(refusal.code()),
                "the stored word and the code are the same"
            );
        }
    }
}
