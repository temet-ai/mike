use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepClass {
    Commit,
    Push,
    Merge,
    Deploy,
    Pr,
    Proceed,
    FileIssue,
    FixFound,
    PickOption,
    RunCommand,
    Credentials,
    Physical,
    Destructive,
    External,
    Unknown,
}

impl StepClass {
    pub fn code(self) -> &'static str {
        match self {
            Self::Commit => "commit",
            Self::Push => "push",
            Self::Merge => "merge",
            Self::Deploy => "deploy",
            Self::Pr => "pr",
            Self::Proceed => "proceed",
            Self::FileIssue => "file_issue",
            Self::FixFound => "fix_found",
            Self::PickOption => "pick_option",
            Self::RunCommand => "run_command",
            Self::Credentials => "credentials",
            Self::Physical => "physical",
            Self::Destructive => "destructive",
            Self::External => "external",
            Self::Unknown => "unknown",
        }
    }

    pub fn is_boundary(self) -> bool {
        matches!(
            self,
            Self::Credentials | Self::Physical | Self::Destructive | Self::External | Self::Unknown
        )
    }

    pub fn directive_class(self) -> Option<&'static str> {
        match self {
            Self::Commit => Some("commit"),
            Self::Push => Some("push"),
            Self::Merge => Some("merge"),
            Self::Deploy => Some("deploy"),
            Self::Pr => Some("pr"),
            Self::Proceed => Some("proceed"),
            Self::FileIssue => Some("file_issue"),
            Self::FixFound => Some("fix_found"),
            Self::PickOption => Some("pick_option"),
            Self::RunCommand => Some("run_command"),
            _ => None,
        }
    }
}

#[cfg(test)]
pub const JC03_CLASSES: [&str; 7] = [
    "commit",
    "push",
    "merge",
    "deploy",
    "pr",
    "proceed",
    "ask_first",
];
#[cfg(test)]
pub const PENDING_JC03_CLASSES: [&str; 4] =
    ["file_issue", "fix_found", "pick_option", "run_command"];

static ASK: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(shall i|should i|want me to|ok to|do you want me to|would you like me to)\b(.*)",
    )
    .ok()
});

const RULES: [(StepClass, &str); 14] = [
    (
        StepClass::Credentials,
        r"(?i)\b(token|credential|credentials|log ?in|password|api key|secret|2fa|otp|sso)\b",
    ),
    (
        StepClass::Destructive,
        r"(?i)(force[- ]push|push --force|push -f\b|rewrite (the )?history|rm -rf|\b(drop|delete|wipe|destroy|revoke|reset --hard)\b)",
    ),
    (
        StepClass::External,
        r"(?i)\b(post|comment|tweet|email|slack|message|contact|pay|buy|purchase|spend)\b",
    ),
    (
        StepClass::Physical,
        r"(?i)\b(device|hardware|plug|unplug|reboot your|your machine|your laptop|physically)\b",
    ),
    (
        StepClass::Pr,
        r"(?i)\b(open|create|raise|file|submit) (a |the )?(pr|pull request)\b|\bpr\b",
    ),
    (
        StepClass::FileIssue,
        r"(?i)\b(file|open|create|raise) (an? |the )?issue\b",
    ),
    (StepClass::Commit, r"(?i)\bcommit\b"),
    (StepClass::Push, r"(?i)\bpush\b"),
    (StepClass::Merge, r"(?i)\bmerge\b"),
    (
        StepClass::Deploy,
        r"(?i)\b(deploy|release|roll (it )?out|ship it)\b",
    ),
    (
        StepClass::FixFound,
        r"(?i)\b(fix|patch|repair) (it|that|this|them|those|the)\b",
    ),
    (
        StepClass::PickOption,
        r"(?i)\b(go with|pick|choose|option [a-z0-9]|approach [a-z0-9])\b",
    ),
    (
        StepClass::RunCommand,
        r"(?i)\b(run|execute|kick off|start) (it|the|a|this)\b",
    ),
    (
        StepClass::Proceed,
        r"(?i)\b(proceed|continue|go ahead|carry on|keep going)\b",
    ),
];

static COMPILED: LazyLock<Vec<(StepClass, Regex)>> = LazyLock::new(|| {
    RULES
        .iter()
        .filter_map(|(class, pattern)| Regex::new(pattern).ok().map(|re| (*class, re)))
        .collect()
});

pub fn asked_sentence(ending: &str) -> Option<String> {
    let re = ASK.as_ref()?;
    ending
        .split(['\n', '.', '!'])
        .rev()
        .map(str::trim)
        .find(|s| re.is_match(s))
        .map(str::to_owned)
}

pub fn classify_sentence(sentence: &str) -> StepClass {
    let asked = ASK
        .as_ref()
        .and_then(|re| re.captures(sentence))
        .and_then(|c| c.get(2))
        .map_or(sentence, |m| m.as_str());
    COMPILED
        .iter()
        .find(|(_, re)| re.is_match(asked))
        .map_or(StepClass::Unknown, |(class, _)| *class)
}

pub fn classify_ending(ending: &str) -> Option<StepClass> {
    asked_sentence(ending).map(|s| classify_sentence(&s))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DirectiveInForce {
    pub id: i64,
    pub class: String,
    pub polarity: String,
    pub text: String,
    pub operator_turn_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Authorisation {
    Authorised(Vec<i64>),
    Forbidden(Vec<i64>),
    NotAuthorised,
}

pub fn authorise(class: StepClass, directives: &[DirectiveInForce]) -> Authorisation {
    let Some(code) = class.directive_class() else {
        return Authorisation::NotAuthorised;
    };
    let forbids: Vec<i64> = directives
        .iter()
        .filter(|d| d.class == "ask_first" || (d.class == code && d.polarity == "forbid"))
        .map(|d| d.id)
        .collect();
    if !forbids.is_empty() {
        return Authorisation::Forbidden(forbids);
    }
    let allows: Vec<i64> = directives
        .iter()
        .filter(|d| d.polarity == "allow" && (d.class == code || d.class == "proceed"))
        .map(|d| d.id)
        .collect();
    if allows.is_empty() {
        Authorisation::NotAuthorised
    } else {
        Authorisation::Authorised(allows)
    }
}
