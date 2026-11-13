#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Ok,
    Note,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub kind: Kind,
    pub text: String,
}

impl Finding {
    pub fn ok(text: impl Into<String>) -> Self {
        Self {
            kind: Kind::Ok,
            text: text.into(),
        }
    }

    pub fn note(text: impl Into<String>) -> Self {
        Self {
            kind: Kind::Note,
            text: text.into(),
        }
    }

    pub fn failed(text: impl Into<String>) -> Self {
        Self {
            kind: Kind::Failed,
            text: text.into(),
        }
    }

    pub fn passed(&self) -> bool {
        self.kind != Kind::Failed
    }
}

pub fn all_passed(findings: &[Finding]) -> bool {
    findings.iter().all(Finding::passed)
}
