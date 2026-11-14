use serde::Serialize;
use serde_json::{Value, json};

pub(crate) const MAX_FILES: usize = 20;
pub(crate) const MAX_LINES: u64 = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Category {
    WrightAuthority,
    Authentication,
    GateConfig,
    Credentials,
    Deploy,
}

impl Category {
    pub(crate) fn describe(self) -> &'static str {
        match self {
            Category::WrightAuthority => "Wright's own authority, limits, measures or toggle",
            Category::Authentication => "authentication",
            Category::GateConfig => "gate, hook or lint configuration",
            Category::Credentials => "credentials",
            Category::Deploy => "a deploy path",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ChangedFile {
    pub(crate) path: String,
    pub(crate) added: Option<u64>,
    pub(crate) removed: Option<u64>,
}

impl ChangedFile {
    fn lines(&self) -> u64 {
        self.added.unwrap_or(0) + self.removed.unwrap_or(0)
    }

    fn binary(&self) -> bool {
        self.added.is_none() || self.removed.is_none()
    }
}

const DEPLOY_PREFIXES: [&str; 6] = [
    "scripts/deploy",
    "scripts/systemd/",
    "scripts/start-proxy",
    "scripts/stop-proxy",
    "scripts/release",
    ".deploy/",
];

const GATE_PREFIXES: [&str; 12] = [
    "scripts/",
    ".githooks/",
    ".claude/",
    ".github/",
    ".semgrep",
    ".cargo/",
    ".config/",
    ".mike/",
    "rust-toolchain",
    ".gitleaks",
    "lefthook",
    ".pre-commit",
];

const GATE_FILES: [&str; 13] = [
    "Makefile",
    "deny.toml",
    "clippy.toml",
    ".clippy.toml",
    "rustfmt.toml",
    ".rustfmt.toml",
    "Cargo.toml",
    "Cargo.lock",
    ".gitattributes",
    ".gitignore",
    ".gitmodules",
    "AGENTS.md",
    "CLAUDE.md",
];

const WRIGHT_PREFIXES: [&str; 4] = [
    "crates/mike/src/wright/",
    "crates/mike/src/store/persistence/wright/",
    "crates/mike/src/api/wright/",
    "crates/mike/src/api/manifest/wright.rs",
];

fn stem(segment: &str) -> &str {
    segment.split('.').next().unwrap_or(segment)
}

fn is_wright(path: &str, segments: &[&str]) -> bool {
    WRIGHT_PREFIXES
        .iter()
        .any(|prefix| path.starts_with(prefix))
        || segments.iter().any(|s| {
            let s = s.to_ascii_lowercase();
            s == "wright" || s.starts_with("wright_") || s.starts_with("wright.")
        })
        || segments.iter().any(|s| stem(s) == "feature_toggles")
        || (segments.contains(&"migrations") && path.to_ascii_lowercase().contains("wright"))
}

fn is_authentication(segments: &[&str]) -> bool {
    segments.iter().any(|segment| {
        let stem = stem(segment).to_ascii_lowercase();
        stem == "auth"
            || stem.starts_with("auth_")
            || stem.starts_with("oauth")
            || stem == "host_guard"
            || stem.starts_with("service_token")
            || stem.starts_with("operator_token")
    })
}

fn is_credentials(path: &str, segments: &[&str]) -> bool {
    let lower = path.to_ascii_lowercase();
    segments.iter().any(|segment| {
        let segment = segment.to_ascii_lowercase();
        segment.starts_with(".env")
            || segment.starts_with("secrets")
            || segment.starts_with("credential")
            || segment == ".git-crypt"
    }) || lower.ends_with(".pem")
        || lower.ends_with(".key")
}

fn is_gate(path: &str, segments: &[&str]) -> bool {
    GATE_PREFIXES.iter().any(|prefix| path.starts_with(prefix))
        || segments
            .iter()
            .any(|s| GATE_FILES.contains(s) || *s == ".claude" || *s == ".githooks")
}

pub(crate) fn category(path: &str) -> Option<Category> {
    let segments: Vec<&str> = path.split('/').collect();
    if DEPLOY_PREFIXES
        .iter()
        .any(|prefix| path.starts_with(prefix))
    {
        return Some(Category::Deploy);
    }
    if is_wright(path, &segments) {
        return Some(Category::WrightAuthority);
    }
    if is_credentials(path, &segments) {
        return Some(Category::Credentials);
    }
    if is_authentication(&segments) {
        return Some(Category::Authentication);
    }
    is_gate(path, &segments).then_some(Category::GateConfig)
}

pub(crate) fn migration_touches_wright(added_lines: &str) -> bool {
    added_lines
        .lines()
        .filter(|line| line.starts_with('+') && !line.starts_with("+++"))
        .any(|line| {
            let line = line.to_ascii_lowercase();
            line.contains("wright") || line.contains("feature_toggles")
        })
}

fn listed(paths: &[&str]) -> String {
    paths
        .iter()
        .map(|path| format!("`{path}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn check(files: &[ChangedFile], migration_diff: &str) -> Result<(), String> {
    for file in files {
        if let Some(category) = category(&file.path) {
            return Err(format!(
                "refused: `{}` touches {}, which a repair may not change",
                file.path,
                category.describe()
            ));
        }
    }
    if migration_touches_wright(migration_diff) {
        return Err(
            "refused: a migration in the patch changes Wright's tables or a feature toggle"
                .to_owned(),
        );
    }
    let binary: Vec<&str> = files
        .iter()
        .filter(|file| file.binary())
        .map(|file| file.path.as_str())
        .collect();
    if !binary.is_empty() {
        return Err(format!(
            "refused: binary changes are not repairs: {}",
            listed(&binary)
        ));
    }
    let lines: u64 = files.iter().map(ChangedFile::lines).sum();
    if files.len() > MAX_FILES || lines > MAX_LINES {
        return Err(format!(
            "refused: the patch changes {} files and {lines} lines; a repair is at most {MAX_FILES} files and {MAX_LINES} lines",
            files.len()
        ));
    }
    Ok(())
}

pub(crate) fn parse_numstat(raw: &str) -> Vec<ChangedFile> {
    raw.split('\0')
        .filter(|entry| !entry.trim().is_empty())
        .filter_map(|entry| {
            let mut fields = entry.trim_start_matches('\n').splitn(3, '\t');
            let added = fields.next()?;
            let removed = fields.next()?;
            let path = fields.next()?;
            Some(ChangedFile {
                path: path.to_owned(),
                added: added.parse().ok(),
                removed: removed.parse().ok(),
            })
        })
        .collect()
}

pub(crate) fn patch_summary(files: &[ChangedFile]) -> Value {
    json!({
        "files": files,
        "added": files.iter().map(|f| f.added.unwrap_or(0)).sum::<u64>(),
        "removed": files.iter().map(|f| f.removed.unwrap_or(0)).sum::<u64>(),
    })
}
