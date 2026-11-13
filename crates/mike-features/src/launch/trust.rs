use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Map, Value};

use super::error::LaunchError;

const TRUST_KEY: &str = "hasTrustDialogAccepted";
const GIT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustOutcome {
    Trusted { target: PathBuf },
    Written { target: PathBuf },
    Untrusted { target: PathBuf, remedy: String },
    Unknown { target: PathBuf, detail: String },
}

#[derive(Debug, Clone)]
pub struct TrustGate {
    config_path: PathBuf,
}

impl TrustGate {
    pub fn new(config_path: PathBuf) -> Self {
        Self { config_path }
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub fn from_home() -> Result<Self, LaunchError> {
        let home = std::env::var("HOME")
            .ok()
            .filter(|h| !h.is_empty())
            .ok_or_else(|| {
                LaunchError::Failed("HOME is not set; cannot find ~/.claude.json".into())
            })?;
        Ok(Self::new(Path::new(&home).join(".claude.json")))
    }

    pub async fn ensure(&self, folder: &Path, write: bool) -> Result<TrustOutcome, LaunchError> {
        let target = trust_target(folder).await;
        let key = target.to_string_lossy().into_owned();
        let mut root = match self.read_config() {
            Ok(root) => root,
            Err(detail) => return Ok(TrustOutcome::Unknown { target, detail }),
        };
        if project_trusted(&root, &key) {
            return Ok(TrustOutcome::Trusted { target });
        }
        if !write {
            return Ok(TrustOutcome::Untrusted {
                remedy: format!("pass --trust, or accept Claude Code's trust dialog once in {key}"),
                target,
            });
        }
        set_trusted(&mut root, &key);
        write_atomic(&self.config_path, &root)?;
        Ok(TrustOutcome::Written { target })
    }

    fn read_config(&self) -> Result<Map<String, Value>, String> {
        let bytes = std::fs::read(&self.config_path)
            .map_err(|e| format!("reading {}: {e}", self.config_path.display()))?;
        match serde_json::from_slice::<Value>(&bytes) {
            Ok(Value::Object(map)) => Ok(map),
            Ok(_) => Err(format!(
                "{} is not a JSON object",
                self.config_path.display()
            )),
            Err(e) => Err(format!("parsing {}: {e}", self.config_path.display())),
        }
    }
}

pub async fn trust_target(folder: &Path) -> PathBuf {
    let resolved = realpath(folder);
    match git_toplevel(&resolved).await {
        Some(top) => realpath(&top),
        None => resolved,
    }
}

fn realpath(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

async fn git_toplevel(dir: &Path) -> Option<PathBuf> {
    let output = tokio::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "--show-toplevel"])
        .stdin(std::process::Stdio::null())
        .output();
    let output = tokio::time::timeout(GIT_TIMEOUT, output).await.ok()?.ok()?;
    if !output.status.success() {
        return None;
    }
    let top = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!top.is_empty()).then(|| PathBuf::from(top))
}

fn project_trusted(root: &Map<String, Value>, key: &str) -> bool {
    root.get("projects")
        .and_then(Value::as_object)
        .and_then(|projects| projects.get(key))
        .and_then(Value::as_object)
        .and_then(|entry| entry.get(TRUST_KEY))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn set_trusted(root: &mut Map<String, Value>, key: &str) {
    let projects = root
        .entry("projects")
        .or_insert_with(|| Value::Object(Map::new()));
    if !projects.is_object() {
        *projects = Value::Object(Map::new());
    }
    let Some(projects) = projects.as_object_mut() else {
        return;
    };
    let entry = projects
        .entry(key)
        .or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    if let Some(entry) = entry.as_object_mut() {
        entry.insert(TRUST_KEY.to_string(), Value::Bool(true));
    }
}

fn write_atomic(path: &Path, root: &Map<String, Value>) -> Result<(), LaunchError> {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut tmp = tempfile::Builder::new()
        .prefix(".claude.json.mike-")
        .tempfile_in(dir)
        .map_err(|e| LaunchError::io(format!("creating a temp file in {}", dir.display()), e))?;
    let bytes = serde_json::to_vec(root)
        .map_err(|e| LaunchError::Failed(format!("encoding {}: {e}", path.display())))?;
    tmp.write_all(&bytes)
        .map_err(|e| LaunchError::io("writing the trust config", e))?;
    tmp.as_file()
        .set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(|e| LaunchError::io("securing the trust config", e))?;
    tmp.persist(path)
        .map_err(|e| LaunchError::io(format!("replacing {}", path.display()), e.error))?;
    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
pub mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn config_with(target: &Path, trusted: Option<bool>) -> (tempfile::TempDir, TrustGate) {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join(".claude.json");
        let mut entry = serde_json::json!({"allowedTools": ["Bash"]});
        if let Some(t) = trusted {
            entry[TRUST_KEY] = Value::Bool(t);
        }
        let doc = serde_json::json!({
            "numStartups": 3,
            "projects": { target.to_string_lossy(): entry, "/elsewhere": {TRUST_KEY: true} }
        });
        std::fs::write(&path, serde_json::to_vec(&doc).unwrap()).unwrap();
        (home, TrustGate::new(path))
    }

    #[tokio::test]
    async fn ensure_trusted_entry_reports_trusted_and_does_not_rewrite() {
        let folder = tempfile::tempdir().unwrap();
        let target = std::fs::canonicalize(folder.path()).unwrap();
        let (home, gate) = config_with(&target, Some(true));
        let before = std::fs::metadata(home.path().join(".claude.json"))
            .unwrap()
            .modified()
            .unwrap();
        let outcome = gate.ensure(folder.path(), true).await.unwrap();
        assert_eq!(outcome, TrustOutcome::Trusted { target });
        let after = std::fs::metadata(home.path().join(".claude.json"))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(before, after);
    }

    #[tokio::test]
    async fn ensure_untrusted_without_write_reports_remedy() {
        let folder = tempfile::tempdir().unwrap();
        let target = std::fs::canonicalize(folder.path()).unwrap();
        let (_home, gate) = config_with(&target, Some(false));
        match gate.ensure(folder.path(), false).await.unwrap() {
            TrustOutcome::Untrusted { target: t, remedy } => {
                assert_eq!(t, target);
                assert!(remedy.contains("--trust"));
            }
            other => panic!("expected Untrusted, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn ensure_write_sets_key_atomically_0600_and_preserves_siblings() {
        let folder = tempfile::tempdir().unwrap();
        let target = std::fs::canonicalize(folder.path()).unwrap();
        let (home, gate) = config_with(&target, None);
        let outcome = gate.ensure(folder.path(), true).await.unwrap();
        assert_eq!(
            outcome,
            TrustOutcome::Written {
                target: target.clone()
            }
        );
        let path = home.path().join(".claude.json");
        let doc: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            doc["projects"][target.to_string_lossy().as_ref()][TRUST_KEY],
            Value::Bool(true)
        );
        assert_eq!(
            doc["projects"][target.to_string_lossy().as_ref()]["allowedTools"][0],
            "Bash"
        );
        assert_eq!(doc["projects"]["/elsewhere"][TRUST_KEY], Value::Bool(true));
        assert_eq!(doc["numStartups"], 3);
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(
            !std::fs::read_dir(home.path()).unwrap().any(|e| e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("mike-"))
        );
        assert_eq!(
            gate.ensure(folder.path(), true).await.unwrap(),
            TrustOutcome::Trusted { target }
        );
    }

    #[tokio::test]
    async fn ensure_missing_config_is_unknown_and_writes_nothing() {
        let folder = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join(".claude.json");
        let gate = TrustGate::new(path.clone());
        match gate.ensure(folder.path(), true).await.unwrap() {
            TrustOutcome::Unknown { detail, .. } => assert!(detail.contains("reading")),
            other => panic!("expected Unknown, got {other:?}"),
        }
        assert!(!path.exists());
        std::fs::write(&path, b"[]").unwrap();
        assert!(matches!(
            gate.ensure(folder.path(), true).await.unwrap(),
            TrustOutcome::Unknown { .. }
        ));
    }

    #[tokio::test]
    async fn trust_target_is_repo_root_for_a_subfolder() {
        let repo = tempfile::tempdir().unwrap();
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(repo.path())
            .arg("init")
            .arg("-q")
            .status()
            .unwrap();
        assert!(status.success());
        let sub = repo.path().join("crates").join("x");
        std::fs::create_dir_all(&sub).unwrap();
        assert_eq!(
            trust_target(&sub).await,
            std::fs::canonicalize(repo.path()).unwrap()
        );
        let plain = tempfile::tempdir().unwrap();
        assert_eq!(
            trust_target(plain.path()).await,
            std::fs::canonicalize(plain.path()).unwrap()
        );
    }

    #[test]
    fn write_atomic_unwritable_dir_is_io_error() {
        let err =
            write_atomic(Path::new("/proc/nonexistent/.claude.json"), &Map::new()).unwrap_err();
        assert!(matches!(err, LaunchError::Io { .. }), "{err}");
    }
}
