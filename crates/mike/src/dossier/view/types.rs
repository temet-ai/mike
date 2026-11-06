use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::store::persistence::PersistError;
use crate::store::persistence::dossier_entries::DossierEntryRow;
use crate::store::persistence::dossier_view::{
    DossierStreamRow, DossierTurnRow, GoalsRow, LedgerAsOfRow, OpenTextRow, SessionHeaderRow,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AsOfMode {
    Knowledge,
    Reconstructed,
}

impl AsOfMode {
    pub fn code(self) -> &'static str {
        match self {
            AsOfMode::Knowledge => "knowledge",
            AsOfMode::Reconstructed => "reconstructed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AsOf {
    pub at: DateTime<Utc>,
    pub mode: AsOfMode,
}

impl AsOf {
    pub fn knowledge(at: DateTime<Utc>) -> Self {
        Self {
            at,
            mode: AsOfMode::Knowledge,
        }
    }

    pub fn reconstructed_for_eval(at: DateTime<Utc>) -> Self {
        Self {
            at,
            mode: AsOfMode::Reconstructed,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SessionSlices {
    pub reactions: bool,
    pub promises: bool,
    pub directives: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SteererSection {
    Ending,
    RecentReactions,
    Directives { classes: Option<Vec<String>> },
    Promises,
    SteeringFacts,
    Asks,
    Corrections,
    SimilarCorrections,
    SimilarDirectives,
    SimilarLessons,
    EarlierNotes,
    TurnOpening,
    EarlierTurns,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Audience {
    Judge {
        turn_id: i64,
    },
    SessionJudge {
        slices: SessionSlices,
    },
    Steerer {
        turn_id: Option<i64>,
        sections: Vec<SteererSection>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AudienceCode {
    Judge,
    SessionJudge,
    Steerer,
}

impl AudienceCode {
    pub fn code(self) -> &'static str {
        match self {
            AudienceCode::Judge => "judge",
            AudienceCode::SessionJudge => "session_judge",
            AudienceCode::Steerer => "steerer",
        }
    }
}

impl Audience {
    pub fn code(&self) -> AudienceCode {
        match self {
            Audience::Judge { .. } => AudienceCode::Judge,
            Audience::SessionJudge { .. } => AudienceCode::SessionJudge,
            Audience::Steerer { .. } => AudienceCode::Steerer,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Budget {
    JudgeSlices,
    Chars(usize),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Needs {
    pub stream: bool,
    pub turns: bool,
    pub ledger: bool,
    pub entries: bool,
    pub goals: bool,
    pub open_turn: Option<i64>,
    pub labels: bool,
    pub verdicts: bool,
    pub steering_facts: Option<i64>,
    pub semantic: Option<i64>,
    pub notes_window: Option<chrono::Duration>,
}

fn default_notes_window(sections: &[SteererSection]) -> Option<chrono::Duration> {
    sections.contains(&SteererSection::EarlierNotes).then(|| {
        crate::steerer::note_policy::NoteLimits::from_settings(
            &crate::settings::SettingsOverlay::default(),
        )
        .window
    })
}

impl Needs {
    fn merged(self, other: Self) -> Self {
        Self {
            stream: self.stream || other.stream,
            turns: self.turns || other.turns,
            ledger: self.ledger || other.ledger,
            entries: self.entries || other.entries,
            goals: self.goals || other.goals,
            open_turn: self.open_turn.or(other.open_turn),
            labels: self.labels || other.labels,
            verdicts: self.verdicts || other.verdicts,
            steering_facts: self.steering_facts.or(other.steering_facts),
            semantic: self.semantic.or(other.semantic),
            notes_window: self.notes_window.or(other.notes_window),
        }
    }

    fn for_section(
        section: &SteererSection,
        turn_id: Option<i64>,
        notes_window: Option<chrono::Duration>,
    ) -> Self {
        let spoken = Self {
            stream: true,
            turns: true,
            ..Self::default()
        };
        match section {
            SteererSection::Ending => Self {
                turns: true,
                open_turn: turn_id,
                ..Self::default()
            },
            SteererSection::RecentReactions => Self {
                verdicts: true,
                ..spoken
            },
            SteererSection::Directives { .. } | SteererSection::Promises => Self {
                entries: true,
                ..spoken
            },
            SteererSection::SteeringFacts => Self {
                steering_facts: turn_id,
                ..Self::default()
            },
            SteererSection::Asks => spoken,
            SteererSection::Corrections => Self {
                labels: true,
                verdicts: true,
                ..spoken
            },
            SteererSection::SimilarCorrections
            | SteererSection::SimilarDirectives
            | SteererSection::SimilarLessons => Self {
                semantic: turn_id,
                ..Self::default()
            },
            SteererSection::EarlierNotes => Self {
                notes_window,
                ..Self::default()
            },
            SteererSection::TurnOpening | SteererSection::EarlierTurns => Self {
                turns: true,
                ..Self::default()
            },
        }
    }

    pub fn for_sections(
        turn_id: Option<i64>,
        sections: &[SteererSection],
        notes_window: Option<chrono::Duration>,
    ) -> Self {
        sections
            .iter()
            .map(|section| Self::for_section(section, turn_id, notes_window))
            .fold(Self::default(), Self::merged)
    }

    pub fn for_inspection(audience: Option<&Audience>) -> Self {
        match audience {
            Some(Audience::Steerer { turn_id, sections }) => Self {
                notes_window: default_notes_window(sections),
                ..Self::everything(*turn_id)
            },
            _ => Self::everything(None),
        }
    }

    pub fn everything(open_turn: Option<i64>) -> Self {
        Self {
            stream: true,
            turns: true,
            ledger: true,
            entries: true,
            goals: true,
            open_turn,
            labels: true,
            verdicts: true,
            steering_facts: None,
            semantic: None,
            notes_window: None,
        }
    }

    pub fn for_audience(audience: &Audience) -> Self {
        match audience {
            Audience::Judge { .. } => Self {
                stream: true,
                turns: true,
                entries: true,
                goals: true,
                labels: true,
                ..Self::default()
            },
            Audience::SessionJudge { slices } => Self {
                stream: true,
                turns: true,
                entries: slices.promises || slices.directives,
                verdicts: slices.reactions,
                ..Self::default()
            },
            Audience::Steerer { turn_id, sections } => Self {
                open_turn: *turn_id,
                steering_facts: *turn_id,
                semantic: *turn_id,
                notes_window: default_notes_window(sections),
                ..Self::everything(None)
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Exclusion {
    pub section: &'static str,
    pub reason: String,
    pub count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reaction {
    Corrected,
    Redirected,
    Accepted,
    NoResponse,
    Unclear,
}

impl Reaction {
    pub fn code(self) -> &'static str {
        match self {
            Reaction::Corrected => "corrected",
            Reaction::Redirected => "redirected",
            Reaction::Accepted => "accepted",
            Reaction::NoResponse => "no_response",
            Reaction::Unclear => "unclear",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "corrected" => Some(Reaction::Corrected),
            "redirected" => Some(Reaction::Redirected),
            "accepted" => Some(Reaction::Accepted),
            "no_response" => Some(Reaction::NoResponse),
            "unclear" => Some(Reaction::Unclear),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Polarity {
    Allow,
    Forbid,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CorrectionSource {
    Label {
        label_id: i64,
        tier: String,
        split: String,
        created_at: DateTime<Utc>,
    },
    LiveVerdict {
        verdict_id: i64,
        reaction: String,
        created_at: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CorrectionItem {
    pub corrected_turn_id: i64,
    pub corrected_created_at: DateTime<Utc>,
    pub corrected_prompt: String,
    pub correcting_request_id: i64,
    pub correcting_text: String,
    pub correcting_at: DateTime<Utc>,
    pub source: CorrectionSource,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReactionItem {
    pub verdict_id: i64,
    pub turn_id: i64,
    pub reaction: String,
    pub delivery: Option<String>,
    pub legacy_label: Option<String>,
    pub confidence: Option<f64>,
    pub rationale: Option<String>,
    pub created_at: DateTime<Utc>,
    pub bundle_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DossierInputs {
    pub session: SessionHeaderRow,
    pub as_of: AsOf,
    pub stream: Vec<DossierStreamRow>,
    pub turns: Vec<DossierTurnRow>,
    pub ledger: Vec<LedgerAsOfRow>,
    pub entries: Vec<DossierEntryRow>,
    pub goals: Option<GoalsRow>,
    pub open_turn: Option<(i64, Vec<OpenTextRow>)>,
    pub corrections: Vec<CorrectionItem>,
    pub reactions: Vec<ReactionItem>,
    pub exclusions: Vec<Exclusion>,
    pub steering_facts: Option<crate::dossier::steering_facts::SteeringFacts>,
    pub semantic: Option<crate::semantic::sections::SemanticSections>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<crate::store::persistence::steering::SessionNote>,
}

#[derive(Debug, thiserror::Error)]
pub enum DossierError {
    #[error("persistence error: {0}")]
    Persist(#[from] PersistError),
    #[error("unknown session {0}")]
    UnknownSession(i64),
    #[error("unknown turn {0}")]
    UnknownTurn(i64),
    #[error("turn {turn_id} is not closed at {at}")]
    TurnNotClosedAt { turn_id: i64, at: DateTime<Utc> },
    #[error("budget does not fit audience {audience:?}")]
    BudgetMismatch { audience: AudienceCode },
}
