use std::ffi::{CString, OsStr};
use std::fs::File;
use std::io::{Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path};

use rustix::fs::{AtFlags, Mode, OFlags};

const SHARED_CREATE_ATTEMPTS: u32 = 8;

pub struct PrivateDirectory {
    file: File,
}

#[derive(PartialEq, Eq)]
pub struct SourceSnapshot {
    pub contents: String,
    device: u64,
    inode: u64,
    mode: u32,
    modified: std::time::SystemTime,
}

impl PrivateDirectory {
    pub fn open(path: &Path) -> std::io::Result<Self> {
        if !path.is_absolute() {
            return Err(invalid("private transaction directory must be absolute"));
        }
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open("/")?;
        for component in path.components() {
            match component {
                Component::RootDir => {}
                Component::Normal(name) => {
                    file = open_at(
                        &file,
                        name,
                        OFlags::RDONLY | OFlags::DIRECTORY,
                        Mode::empty(),
                    )?;
                }
                _ => return Err(invalid("private transaction directory must be normalized")),
            }
        }
        let metadata = file.metadata()?;
        if metadata.uid() != rustix::process::geteuid().as_raw() || metadata.mode() & 0o077 != 0 {
            return Err(invalid(
                "transaction directory must be private and owned by this user",
            ));
        }
        Ok(Self { file })
    }

    pub fn still_selected(&self, path: &Path) -> std::io::Result<()> {
        let current = Self::open(path)?.file.metadata()?;
        let original = self.file.metadata()?;
        if current.dev() != original.dev() || current.ino() != original.ino() {
            return Err(invalid(
                "selected configuration directory changed; retry the update",
            ));
        }
        Ok(())
    }

    pub fn lock_file(&self, name: &OsStr) -> std::io::Result<File> {
        let file = self.create_or_open_shared(name)?;
        let metadata = file.metadata()?;
        require_regular_owned(&metadata)?;
        if metadata.mode() & 0o777 != 0o600 {
            return Err(invalid("configuration write lock must have mode 0600"));
        }
        Ok(file)
    }

    fn create_or_open_shared(&self, name: &OsStr) -> std::io::Result<File> {
        let mut attempts_left = SHARED_CREATE_ATTEMPTS;
        loop {
            match open_at(
                &self.file,
                name,
                OFlags::RDWR | OFlags::CREATE,
                owner_read_write(),
            ) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound && attempts_left > 1 => {
                    attempts_left -= 1;
                }
                result => return result,
            }
        }
    }

    pub fn same_file(&self, name: &OsStr, expected: &File) -> std::io::Result<()> {
        let current = open_at(&self.file, name, OFlags::RDONLY, Mode::empty())?.metadata()?;
        let expected = expected.metadata()?;
        require_regular_owned(&current)?;
        if current.mode() & 0o777 != 0o600 {
            return Err(invalid("configuration write lock permissions changed"));
        }
        if current.dev() != expected.dev() || current.ino() != expected.ino() {
            return Err(invalid(
                "configuration write lock changed; retry the update",
            ));
        }
        Ok(())
    }

    pub fn read(&self, name: &OsStr) -> std::io::Result<Option<SourceSnapshot>> {
        let mut file = match open_at(&self.file, name, OFlags::RDONLY, Mode::empty()) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let metadata = file.metadata()?;
        require_regular_owned(&metadata)?;
        if metadata.mode() & 0o222 == 0 {
            return Err(invalid(
                "selected configuration is read-only; previous configuration retained",
            ));
        }
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let snapshot = SourceSnapshot::new(contents, &metadata)?;
        if snapshot != SourceSnapshot::new(snapshot.contents.clone(), &file.metadata()?)? {
            return Err(invalid(
                "selected configuration changed while reading; retry the update",
            ));
        }
        Ok(Some(snapshot))
    }

    pub fn create_new(&self, name: &OsStr, contents: &str) -> std::io::Result<()> {
        let mut file = open_at(
            &self.file,
            name,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL,
            owner_read_write(),
        )?;
        let result = (|| {
            file.write_all(contents.as_bytes())?;
            file.sync_all()?;
            self.file.sync_all()
        })();
        if result.is_err() {
            let name = single_name(name)?;
            let _ = rustix::fs::unlinkat(&self.file, name.as_c_str(), AtFlags::empty());
        }
        result
    }

    pub fn replace_checked(
        &self,
        name: &OsStr,
        contents: &str,
        check: impl FnOnce() -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        let staged = format!(".mike-config-staged-{}", uuid::Uuid::new_v4());
        self.create_new(OsStr::new(&staged), contents)?;
        let result = check().and_then(|()| self.rename(OsStr::new(&staged), name));
        if result.is_err() {
            let name = single_name(OsStr::new(&staged))?;
            let _ = rustix::fs::unlinkat(&self.file, name.as_c_str(), AtFlags::empty());
        }
        result
    }

    fn rename(&self, from: &OsStr, to: &OsStr) -> std::io::Result<()> {
        let from = single_name(from)?;
        let to = single_name(to)?;
        rustix::fs::renameat(&self.file, from.as_c_str(), &self.file, to.as_c_str())?;
        self.file.sync_all()
    }
}

impl SourceSnapshot {
    fn new(contents: String, metadata: &std::fs::Metadata) -> std::io::Result<Self> {
        Ok(Self {
            contents,
            device: metadata.dev(),
            inode: metadata.ino(),
            mode: metadata.mode(),
            modified: metadata.modified()?,
        })
    }
}

fn require_regular_owned(metadata: &std::fs::Metadata) -> std::io::Result<()> {
    if !metadata.is_file()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.nlink() != 1
    {
        return Err(invalid(
            "configuration transaction requires a regular single-link file owned by this user",
        ));
    }
    Ok(())
}

fn single_name(name: &OsStr) -> std::io::Result<CString> {
    if name.as_bytes().contains(&b'/') || name == "." || name == ".." {
        return Err(invalid(
            "configuration transaction requires a single file name",
        ));
    }
    CString::new(name.as_bytes()).map_err(|_| invalid("invalid transaction file name"))
}

fn owner_read_write() -> Mode {
    Mode::RUSR | Mode::WUSR
}

fn open_at(directory: &File, name: &OsStr, flags: OFlags, mode: Mode) -> std::io::Result<File> {
    let name = single_name(name)?;
    let fd = rustix::fs::openat(
        directory,
        name.as_c_str(),
        flags | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
        mode,
    )?;
    Ok(File::from(fd))
}

fn invalid(message: &'static str) -> std::io::Error {
    std::io::Error::other(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn failed_precommit_check_removes_private_staging_and_keeps_original() -> std::io::Result<()> {
        let temp = tempfile::tempdir()?;
        let path = std::fs::canonicalize(temp.path())?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
        let directory = PrivateDirectory::open(&path)?;
        directory.create_new(OsStr::new("config.toml"), "persist_enabled=false")?;
        assert!(
            directory
                .replace_checked(OsStr::new("config.toml"), "persist_enabled=true", || Err(
                    std::io::Error::other("changed input")
                ))
                .is_err()
        );
        assert_eq!(
            std::fs::read_to_string(path.join("config.toml"))?,
            "persist_enabled=false"
        );
        assert_eq!(std::fs::read_dir(&path)?.count(), 1);
        Ok(())
    }
}
