#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Ok,
    Note,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Finding {
    pub(crate) kind: Kind,
    pub(crate) text: String,
}

impl Finding {
    pub(crate) fn ok(text: impl Into<String>) -> Self {
        Self {
            kind: Kind::Ok,
            text: text.into(),
        }
    }

    pub(crate) fn note(text: impl Into<String>) -> Self {
        Self {
            kind: Kind::Note,
            text: text.into(),
        }
    }

    pub(crate) fn failed(text: impl Into<String>) -> Self {
        Self {
            kind: Kind::Failed,
            text: text.into(),
        }
    }

    pub(crate) fn passed(&self) -> bool {
        self.kind != Kind::Failed
    }
}

pub(crate) fn all_passed(findings: &[Finding]) -> bool {
    findings.iter().all(Finding::passed)
}
