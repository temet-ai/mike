use serde::Serialize;

use crate::dossier::as_of::{AsOf, AsOfMode};

pub const RETRIEVAL_VERSION: &str = "retrieval-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuerySubject {
    ClosedTurn(i64),
    OpenTurn {
        session_id: i64,
        prompt_text: String,
        ending_text: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Audience {
    Judge,
    Steerer,
    LabelFree,
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievedKind {
    Correction,
    Exchange,
    Directive,
    Lesson,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Operator,
    Project,
    Session,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticQuery {
    pub subject: QuerySubject,
    pub audience: Audience,
    pub kinds: Vec<RetrievedKind>,
    pub scope: Scope,
    pub as_of: AsOf,
    pub k: usize,
    pub offset: usize,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum QueryError {
    #[error("audience {audience} may not see kind {kind}")]
    KindNotAllowed {
        audience: &'static str,
        kind: &'static str,
    },
    #[error("a live consumer may not read in reconstructed mode (audience {0})")]
    ReconstructedForLive(&'static str),
    #[error("no kinds requested")]
    NoKinds,
}

impl Audience {
    pub fn code(self) -> &'static str {
        match self {
            Self::Judge => "judge",
            Self::Steerer => "steerer",
            Self::LabelFree => "label_free",
            Self::Internal => "internal",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "judge" => Some(Self::Judge),
            "steerer" => Some(Self::Steerer),
            "label_free" | "label-free" => Some(Self::LabelFree),
            "internal" => Some(Self::Internal),
            _ => None,
        }
    }

    pub fn allowed_kinds(self) -> &'static [RetrievedKind] {
        match self {
            Self::Judge => &[RetrievedKind::Correction, RetrievedKind::Exchange],
            Self::Steerer => &[
                RetrievedKind::Correction,
                RetrievedKind::Exchange,
                RetrievedKind::Directive,
                RetrievedKind::Lesson,
            ],
            Self::LabelFree => &[RetrievedKind::Exchange],
            Self::Internal => &[RetrievedKind::Correction],
        }
    }
}

impl RetrievedKind {
    pub fn code(self) -> &'static str {
        match self {
            Self::Correction => "correction",
            Self::Exchange => "exchange",
            Self::Directive => "directive",
            Self::Lesson => "lesson",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "correction" => Some(Self::Correction),
            "exchange" => Some(Self::Exchange),
            "directive" => Some(Self::Directive),
            "lesson" => Some(Self::Lesson),
            _ => None,
        }
    }

    pub fn pairs(self) -> &'static [&'static str] {
        match self {
            Self::Correction => &super::floors::SIMILAR_CORRECTIONS,
            Self::Exchange => &super::floors::LABEL_FREE_EXCHANGES,
            Self::Directive => &super::floors::DIRECTIVES,
            Self::Lesson => &super::floors::LESSONS,
        }
    }
}

impl Scope {
    pub fn code(self) -> &'static str {
        match self {
            Self::Operator => "operator",
            Self::Project => "project",
            Self::Session => "session",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "operator" => Some(Self::Operator),
            "project" => Some(Self::Project),
            "session" => Some(Self::Session),
            _ => None,
        }
    }
}

pub fn validate(q: &SemanticQuery, live_consumer: bool) -> Result<(), QueryError> {
    if q.kinds.is_empty() {
        return Err(QueryError::NoKinds);
    }
    let allowed = q.audience.allowed_kinds();
    if let Some(kind) = q.kinds.iter().find(|k| !allowed.contains(k)) {
        return Err(QueryError::KindNotAllowed {
            audience: q.audience.code(),
            kind: kind.code(),
        });
    }
    if live_consumer && q.as_of.mode == AsOfMode::Reconstructed {
        return Err(QueryError::ReconstructedForLive(q.audience.code()));
    }
    Ok(())
}

pub fn default_kinds(audience: Audience) -> Vec<RetrievedKind> {
    audience.allowed_kinds().to_vec()
}
