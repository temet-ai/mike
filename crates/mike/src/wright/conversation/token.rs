use std::io::Read as _;
use std::path::{Path, PathBuf};

use axum::http::HeaderMap;
use axum::http::header::AUTHORIZATION;
use secrecy::{ExposeSecret, SecretString};
use sha2::{Digest, Sha256};

pub(crate) const FILE_NAME: &str = "operator-token";
const RANDOM_BYTES: usize = 32;

pub(crate) struct OperatorToken(SecretString);

impl OperatorToken {
    pub(crate) fn from_text(text: &str) -> Option<Self> {
        let text = text.trim();
        (!text.is_empty()).then(|| Self(SecretString::new(text.to_owned().into())))
    }

    pub(crate) fn generate() -> std::io::Result<Self> {
        let mut bytes = [0_u8; RANDOM_BYTES];
        std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
        Ok(Self(SecretString::new(hex::encode(bytes).into())))
    }

    pub(crate) fn expose(&self) -> &str {
        self.0.expose_secret()
    }

    pub(crate) fn matches(&self, candidate: &str) -> bool {
        let expected = Sha256::digest(self.expose().as_bytes());
        let given = Sha256::digest(candidate.as_bytes());
        let difference = expected
            .iter()
            .zip(given.iter())
            .fold(0_u8, |acc, (a, b)| acc | (a ^ b));
        std::hint::black_box(difference) == 0
    }
}

pub(crate) fn default_path() -> Result<PathBuf, String> {
    crate::config::paths::InstallationPaths::resolve(None)
        .map(|paths| paths.state_dir.join(FILE_NAME))
        .map_err(|e| format!("the mike state directory is unknown: {e}"))
}

pub(crate) fn load(path: &Path) -> std::io::Result<Option<OperatorToken>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(OperatorToken::from_text(&text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

#[cfg(unix)]
fn tighten(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = std::fs::metadata(path)?.permissions().mode() & 0o777;
    if mode != 0o600 {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn tighten(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

pub(crate) fn ensure(path: &Path) -> std::io::Result<OperatorToken> {
    if std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err(std::io::Error::other(format!(
            "{} is a symlink; the operator token must be a plain file",
            path.display()
        )));
    }
    if let Some(token) = load(path)? {
        tighten(path)?;
        return Ok(token);
    }
    if let Some(dir) = path.parent() {
        crate::config::write::ensure_private_directory(dir).map_err(std::io::Error::other)?;
    }
    let token = OperatorToken::generate()?;
    crate::owner_only_file::write(path, token.expose())?;
    Ok(token)
}

pub(crate) fn bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(AUTHORIZATION)?.to_str().ok()?.trim();
    let (scheme, token) = value.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then(|| token.trim())
        .filter(|token| !token.is_empty())
}

pub(crate) fn authorize(
    headers: &HeaderMap,
    expected: Option<&OperatorToken>,
) -> Result<(), String> {
    let Some(expected) = expected else {
        return Err(format!(
            "mike has no operator token yet; it creates {FILE_NAME} in its state directory when `mike serve` starts"
        ));
    };
    match bearer(headers) {
        None => Err("this route needs `Authorization: Bearer <operator token>`; run `mike wright pair` to pair the page".to_owned()),
        Some(given) if expected.matches(given) => Ok(()),
        Some(_) => Err("the Bearer token is not the operator token; run `mike wright pair` to pair the page again".to_owned()),
    }
}
