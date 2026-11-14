use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use mike_code::{RuleSet, sgconfig_rule_dirs};
use serde::Serialize;

const RECHECK: Duration = Duration::from_secs(30);
const MAX_FILES: usize = 200;
const MAX_BYTES: u64 = 1024 * 1024;
pub const MIKE_RULE_DIR: &str = ".mike/code-rules";

#[derive(Clone)]
pub enum ActiveRules {
    Builtin(&'static RuleSet),
    Project(Arc<RuleSet>),
}

impl ActiveRules {
    pub fn get(&self) -> &RuleSet {
        match self {
            ActiveRules::Builtin(rules) => rules,
            ActiveRules::Project(rules) => rules,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct ProjectRulesStatus {
    pub checkout: String,
    pub files: Vec<String>,
    pub rules: Vec<String>,
    pub version: Option<String>,
    pub error: Option<String>,
}

type Signature = Vec<(PathBuf, u64, Option<SystemTime>)>;

struct Entry {
    checked: Instant,
    signature: Signature,
    rules: ActiveRules,
}

pub struct ProjectRules {
    builtin: &'static RuleSet,
    entries: HashMap<String, Entry>,
}

fn statuses() -> &'static Mutex<HashMap<String, ProjectRulesStatus>> {
    static STATUS: OnceLock<Mutex<HashMap<String, ProjectRulesStatus>>> = OnceLock::new();
    STATUS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn project_rules_status() -> Vec<ProjectRulesStatus> {
    let mut all: Vec<ProjectRulesStatus> = statuses()
        .lock()
        .map(|map| map.values().cloned().collect())
        .unwrap_or_default();
    all.sort_by(|a, b| a.checkout.cmp(&b.checkout));
    all
}

fn set_status(status: ProjectRulesStatus) {
    if let Ok(mut map) = statuses().lock() {
        if status.files.is_empty() && status.error.is_none() {
            map.remove(&status.checkout);
        } else {
            map.insert(status.checkout.clone(), status);
        }
    }
}

impl ProjectRules {
    pub fn new(builtin: &'static RuleSet) -> Self {
        Self {
            builtin,
            entries: HashMap::new(),
        }
    }

    pub async fn for_checkout(&mut self, root: &str) -> ActiveRules {
        if let Some(entry) = self.entries.get(root)
            && entry.checked.elapsed() < RECHECK
        {
            return entry.rules.clone();
        }
        let previous = self.entries.get(root).map(|e| e.signature.clone());
        let root_owned = root.to_string();
        let builtin = self.builtin;
        let loaded = tokio::task::spawn_blocking(move || load(&root_owned, previous, builtin))
            .await
            .ok()
            .flatten();
        let entry = match loaded {
            Some((signature, rules)) => Entry {
                checked: Instant::now(),
                signature,
                rules,
            },
            None => match self.entries.remove(root) {
                Some(mut entry) => {
                    entry.checked = Instant::now();
                    entry
                }
                None => Entry {
                    checked: Instant::now(),
                    signature: Vec::new(),
                    rules: ActiveRules::Builtin(self.builtin),
                },
            },
        };
        let rules = entry.rules.clone();
        self.entries.insert(root.to_string(), entry);
        rules
    }
}

fn load(
    root: &str,
    previous: Option<Signature>,
    builtin: &'static RuleSet,
) -> Option<(Signature, ActiveRules)> {
    let base = Path::new(root);
    let files = rule_files(base);
    let signature = signature_of(&files);
    if previous.as_ref() == Some(&signature) {
        return None;
    }
    let names: Vec<String> = files.iter().map(|p| relative(base, p)).collect();
    let mut status = ProjectRulesStatus {
        checkout: root.to_string(),
        files: names.clone(),
        ..ProjectRulesStatus::default()
    };
    if files.is_empty() {
        set_status(status);
        return Some((signature, ActiveRules::Builtin(builtin)));
    }
    let texts: Vec<(String, String)> = files
        .iter()
        .zip(names)
        .filter_map(|(path, name)| std::fs::read_to_string(path).ok().map(|t| (name, t)))
        .collect();
    let rules = build(root, &texts, builtin, &mut status);
    set_status(status);
    Some((signature, rules))
}

fn build(
    root: &str,
    texts: &[(String, String)],
    builtin: &'static RuleSet,
    status: &mut ProjectRulesStatus,
) -> ActiveRules {
    match RuleSet::with_project(texts) {
        Ok(rules) => {
            let mut ids: Vec<String> = rules.project_ids().map(str::to_string).collect();
            ids.sort();
            status.rules = ids;
            status.version = Some(rules.version().to_string());
            log_loaded(root, status.rules.len(), rules.version());
            ActiveRules::Project(Arc::new(rules))
        }
        Err(err) => {
            log_load_failed(root, &err);
            status.error = Some(err.to_string());
            ActiveRules::Builtin(builtin)
        }
    }
}

fn relative(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

pub fn rule_dirs(base: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(text) = std::fs::read_to_string(base.join("sgconfig.yml")) {
        match sgconfig_rule_dirs(&text) {
            Ok(found) => dirs.extend(found.iter().map(|d| base.join(d))),
            Err(err) => {
                tracing::warn!(checkout = %base.display(), error = %err, "code scan: sgconfig.yml could not be read");
            }
        }
    }
    let mike = base.join(MIKE_RULE_DIR);
    if mike.is_dir() {
        dirs.push(mike);
    }
    dirs.retain(|dir| dir.starts_with(base));
    dirs
}

fn rule_files(base: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = rule_dirs(base);
    let mut bytes = 0;
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                stack.push(path);
            } else if kind.is_file() && is_yaml(&path) {
                bytes += entry.metadata().map_or(0, |m| m.len());
                if files.len() >= MAX_FILES || bytes > MAX_BYTES {
                    break;
                }
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn is_yaml(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("yml" | "yaml")
    )
}

fn signature_of(files: &[PathBuf]) -> Signature {
    files
        .iter()
        .map(|path| {
            let meta = std::fs::metadata(path).ok();
            let len = meta.as_ref().map_or(0, std::fs::Metadata::len);
            let modified = meta.and_then(|m| m.modified().ok());
            (path.clone(), len, modified)
        })
        .collect()
}

fn log_loaded(root: &str, rules: usize, version: &str) {
    tracing::info!(checkout = %root, rules, version, "code scan: project rules loaded");
}

fn log_load_failed(root: &str, err: &mike_code::RuleError) {
    tracing::warn!(checkout = %root, error = %err, "code scan: the project's rules did not load, so only the built-in rules run");
}
