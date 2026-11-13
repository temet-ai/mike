use std::path::{Path, PathBuf};

use secrecy::{ExposeSecret, SecretString};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const HEADER: &str = "x-mike-service-token";

const FILE_PREFIX: &str = "service-token";

#[derive(Clone)]
pub struct ServiceToken(SecretString);

impl ServiceToken {
    pub fn generate() -> Self {
        let value = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        Self(SecretString::new(value.into()))
    }

    pub fn expose(&self) -> &str {
        self.0.expose_secret()
    }

    pub fn matches(&self, candidate: &str) -> bool {
        digest(candidate) == digest(self.expose())
    }
}

fn digest(value: &str) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hasher.finalize().to_vec()
}

pub fn file_in(state_dir: &Path, inspect_port: u16) -> PathBuf {
    state_dir.join(format!("{FILE_PREFIX}.{inspect_port}"))
}

pub fn publish(
    token: &ServiceToken,
    state_dir: &Path,
    inspect_port: u16,
) -> std::io::Result<PathBuf> {
    crate::config::write::ensure_private_directory(state_dir).map_err(std::io::Error::other)?;
    let path = file_in(state_dir, inspect_port);
    if std::fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(std::io::Error::other(
            "service token target must not be a symlink",
        ));
    }
    crate::owner_only_file::write(&path, token.expose())?;
    Ok(path)
}

pub fn publish_to_state_dir(token: &ServiceToken, state_dir: &Path, inspect_port: u16) {
    match publish(token, state_dir, inspect_port) {
        Ok(path) => log_published(&path),
        Err(e) => log_not_published(&e),
    }
}

fn log_published(path: &Path) {
    tracing::info!(path = %path.display(), "service token published");
}

fn log_not_published(e: &std::io::Error) {
    tracing::warn!(error = %e, "service token not published");
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn generated_tokens_are_long_and_differ() {
        let a = ServiceToken::generate();
        let b = ServiceToken::generate();
        assert_eq!(a.expose().len(), 64);
        assert_ne!(a.expose(), b.expose());
    }

    #[test]
    fn only_the_exact_token_matches() {
        let token = ServiceToken::generate();
        let exact = token.expose().to_owned();
        assert!(token.matches(&exact));
        assert!(!token.matches(""));
        assert!(!token.matches(&exact[..63]));
        assert!(!token.matches(&format!("{exact}0")));
    }

    #[test]
    fn the_file_name_carries_the_inspect_port() {
        let path = file_in(Path::new("/state/mike"), 9989);
        assert_eq!(path, PathBuf::from("/state/mike/service-token.9989"));
    }

    #[cfg(unix)]
    #[test]
    fn the_published_file_is_owner_only_and_holds_the_token() -> anyhow::Result<()> {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir()?;
        let root = dir.path().canonicalize()?;
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
        let token = ServiceToken::generate();
        let path = publish(&token, &root, 4321)?;
        assert_eq!(std::fs::read_to_string(&path)?, token.expose());
        assert_eq!(
            std::fs::metadata(&path)?.permissions().mode() & 0o777,
            0o600
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn republishing_tightens_a_file_left_world_readable() -> anyhow::Result<()> {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir()?;
        let root = dir.path().canonicalize()?;
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
        let path = file_in(&root, 4321);
        std::fs::write(&path, "stale")?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))?;
        let token = ServiceToken::generate();
        publish(&token, &root, 4321)?;
        assert_eq!(std::fs::read_to_string(&path)?, token.expose());
        assert_eq!(
            std::fs::metadata(&path)?.permissions().mode() & 0o777,
            0o600
        );
        Ok(())
    }
}
