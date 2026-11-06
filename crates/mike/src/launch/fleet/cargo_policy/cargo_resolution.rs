use std::ffi::{CString, OsStr, OsString};
use std::fs::{File, Metadata, OpenOptions};
use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

pub const PROTOCOL_MARKER: &str = "mike-cargo-shim-v1\n";
pub const MARKER_NAME: &str = ".mike-cargo-shim";

#[derive(Debug, thiserror::Error)]
pub enum PathError {
    #[error("unsafe Cargo shim path {path:?}: {reason}")]
    Unsafe { path: PathBuf, reason: &'static str },
    #[error("could not inspect Cargo shim path {path:?}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("could not reconstruct the Cargo search PATH: {0}")]
    Join(#[from] std::env::JoinPathsError),
    #[error("no executable Cargo remains on the caller's search PATH")]
    MissingCargo,
    #[error("Cargo resolves back to a Mike shim: {0:?}")]
    RecursiveCargo(PathBuf),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileIdentity {
    device: u64,
    inode: u64,
}

impl FileIdentity {
    fn of(metadata: &Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ShimDirectory {
    root: PathBuf,
    root_identity: FileIdentity,
    path: PathBuf,
    identity: FileIdentity,
    cargo: FileIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCargo {
    pub executable: PathBuf,
    pub child_path: Option<OsString>,
}

fn unsafe_path(path: &Path, reason: &'static str) -> PathError {
    PathError::Unsafe {
        path: path.to_path_buf(),
        reason,
    }
}

fn io_error(path: &Path, source: std::io::Error) -> PathError {
    PathError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn private_metadata(path: &Path, directory: bool) -> Result<Metadata, PathError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| io_error(path, error))?;
    let kind_matches = if directory {
        metadata.is_dir()
    } else {
        metadata.is_file()
    };
    if !kind_matches || metadata.uid() != unsafe { libc::geteuid() } {
        return Err(unsafe_path(
            path,
            "expected an owner-held ordinary file or directory",
        ));
    }
    if metadata.mode() & 0o077 != 0 || (!directory && metadata.nlink() != 1) {
        return Err(unsafe_path(
            path,
            "private permissions and unshared regular files are required",
        ));
    }
    Ok(metadata)
}

fn read_marker(path: &Path) -> Result<(), PathError> {
    let marker = path.join(MARKER_NAME);
    let expected = private_metadata(&marker, false)?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(&marker)
        .map_err(|error| io_error(&marker, error))?;
    verify_open_identity(&file, &expected, &marker)?;
    let mut bytes = Vec::new();
    file.take((PROTOCOL_MARKER.len() + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| io_error(&marker, error))?;
    if bytes != PROTOCOL_MARKER.as_bytes() {
        return Err(unsafe_path(&marker, "unrecognized shim protocol marker"));
    }
    Ok(())
}

fn verify_open_identity(file: &File, expected: &Metadata, path: &Path) -> Result<(), PathError> {
    let actual = file.metadata().map_err(|error| io_error(path, error))?;
    if !actual.is_file()
        || actual.uid() != unsafe { libc::geteuid() }
        || actual.nlink() != 1
        || actual.mode() & 0o077 != 0
    {
        return Err(unsafe_path(
            path,
            "opened marker is not a private unshared regular file",
        ));
    }
    if FileIdentity::of(expected) != FileIdentity::of(&actual) {
        return Err(unsafe_path(path, "file identity changed during validation"));
    }
    Ok(())
}

impl ShimDirectory {
    pub fn verify(root: &Path, directory: &Path) -> Result<Self, PathError> {
        let canonical_root = root.canonicalize().map_err(|error| io_error(root, error))?;
        if !root.is_absolute() || root != canonical_root {
            return Err(unsafe_path(
                root,
                "the shared domain must already be canonical and absolute",
            ));
        }
        let root_metadata = private_metadata(root, true)?;
        let path = directory
            .canonicalize()
            .map_err(|error| io_error(directory, error))?;
        if path == root || !path.starts_with(root) {
            return Err(unsafe_path(
                directory,
                "shim directory must be inside the pinned private domain",
            ));
        }
        let metadata = private_metadata(&path, true)?;
        read_marker(&path)?;
        let cargo_path = path.join("cargo");
        let cargo = private_metadata(&cargo_path, false)?;
        if cargo.mode() & 0o111 == 0 {
            return Err(unsafe_path(
                &cargo_path,
                "the generated Cargo launcher is not executable",
            ));
        }
        Ok(Self {
            root: root.to_path_buf(),
            root_identity: FileIdentity::of(&root_metadata),
            path,
            identity: FileIdentity::of(&metadata),
            cargo: FileIdentity::of(&cargo),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn revalidate(&self) -> Result<(), PathError> {
        let current = Self::verify(&self.root, &self.path)?;
        if current.root_identity != self.root_identity
            || current.identity != self.identity
            || current.cargo != self.cargo
        {
            return Err(unsafe_path(&self.path, "a prepared shim was replaced"));
        }
        Ok(())
    }
}

fn at_cwd(path: &Path, cwd: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

fn is_shim(path: &Path, shims: &[ShimDirectory]) -> bool {
    std::fs::metadata(path).ok().is_some_and(|metadata| {
        metadata.is_dir()
            && shims
                .iter()
                .any(|shim| FileIdentity::of(&metadata) == shim.identity)
    })
}

pub fn without_mike_shims(
    path: &OsStr,
    cwd: &Path,
    shims: &[ShimDirectory],
) -> Result<Option<OsString>, PathError> {
    if !cwd.is_absolute() {
        return Err(unsafe_path(
            cwd,
            "an absolute invocation working directory is required",
        ));
    }
    for shim in shims {
        shim.revalidate()?;
    }
    let retained: Vec<PathBuf> = std::env::split_paths(path)
        .filter(|entry| !is_shim(&at_cwd(entry, cwd), shims))
        .collect();
    if retained.is_empty() {
        Ok(None)
    } else {
        Ok(Some(std::env::join_paths(retained)?))
    }
}

fn executable(path: &Path) -> Option<Metadata> {
    let metadata = std::fs::metadata(path)
        .ok()
        .filter(|metadata| metadata.is_file())?;
    let name = CString::new(path.as_os_str().as_bytes()).ok()?;
    (unsafe { libc::access(name.as_ptr(), libc::X_OK) } == 0).then_some(metadata)
}

pub fn resolve_cargo(
    path: &OsStr,
    cwd: &Path,
    shims: &[ShimDirectory],
) -> Result<ResolvedCargo, PathError> {
    let child_path = without_mike_shims(path, cwd, shims)?;
    let Some(search) = child_path.as_deref() else {
        return Err(PathError::MissingCargo);
    };
    for entry in std::env::split_paths(search) {
        let candidate = at_cwd(&entry, cwd).join("cargo");
        let Some(metadata) = executable(&candidate) else {
            continue;
        };
        if shims
            .iter()
            .any(|shim| shim.cargo == FileIdentity::of(&metadata))
        {
            return Err(PathError::RecursiveCargo(candidate));
        }
        return Ok(ResolvedCargo {
            executable: candidate,
            child_path: child_path.clone(),
        });
    }
    Err(PathError::MissingCargo)
}

#[cfg(feature = "test-seams")]
#[path = "cargo_resolution_cases.rs"]
pub mod cases;
