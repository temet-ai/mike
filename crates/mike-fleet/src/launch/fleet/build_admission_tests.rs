use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{AdmissionError, LOCK_NAME, acquire};
use tokio_util::sync::CancellationToken;

const FIXTURE_ROOT: &str = "MIKE_BUILD_ADMISSION_FIXTURE_ROOT";
const FIXTURE_KIND: &str = "MIKE_BUILD_ADMISSION_FIXTURE_KIND";
const FIXTURE_LIMIT: Duration = Duration::from_secs(10);

fn private_root() -> anyhow::Result<tempfile::TempDir> {
    let root = tempfile::tempdir()?;
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700))?;
    Ok(root)
}

async fn wait_for_file(path: &Path) -> anyhow::Result<()> {
    tokio::time::timeout(FIXTURE_LIMIT, async {
        while !path.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    Ok(())
}

fn child_fixture(root: &Path, kind: &str) -> anyhow::Result<tokio::process::Child> {
    let name = format!("{}::process_fixture", module_path!());
    let (_, name) = name
        .split_once("::")
        .ok_or_else(|| anyhow::anyhow!("no crate path"))?;
    Ok(tokio::process::Command::new(std::env::current_exe()?)
        .args(["--exact", name, "--nocapture"])
        .env(FIXTURE_ROOT, root)
        .env(FIXTURE_KIND, kind)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()?)
}

async fn finish_fixture(mut child: tokio::process::Child, root: &Path) -> anyhow::Result<()> {
    std::fs::write(root.join("release"), b"release")?;
    let status = tokio::time::timeout(FIXTURE_LIMIT, child.wait()).await??;
    anyhow::ensure!(status.success(), "owned test fixture failed: {status}");
    Ok(())
}

#[test]
fn process_fixture() -> anyhow::Result<()> {
    let Some(root) = std::env::var_os(FIXTURE_ROOT).map(PathBuf::from) else {
        return Ok(());
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        std::fs::write(root.join("attempting"), b"attempting")?;
        let cancellation = CancellationToken::new();
        let _permit = match std::env::var(FIXTURE_KIND)?.as_str() {
            "managed" => Some(acquire(&root, &cancellation).await?),
            "unmanaged" => None,
            other => anyhow::bail!("unknown test fixture {other}"),
        };
        std::fs::write(root.join("entered"), b"entered")?;
        wait_for_file(&root.join("release")).await
    })
}

#[tokio::test]
async fn independent_process_waits_for_owner_release_without_replacing_lock() -> anyhow::Result<()>
{
    let root = private_root()?;
    let path = root.path().canonicalize()?;
    let cancellation = CancellationToken::new();
    let held = acquire(&path, &cancellation).await?;
    let before = std::fs::metadata(path.join(LOCK_NAME))?;
    let child = child_fixture(&path, "managed")?;
    wait_for_file(&path.join("attempting")).await?;
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(!path.join("entered").exists());
    drop(held);
    wait_for_file(&path.join("entered")).await?;
    finish_fixture(child, &path).await?;
    let after = std::fs::metadata(path.join(LOCK_NAME))?;
    assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
    let _again = acquire(&path, &cancellation).await?;
    Ok(())
}

#[tokio::test]
async fn process_exit_releases_its_permit_for_the_next_owner() -> anyhow::Result<()> {
    let root = private_root()?;
    let path = root.path().canonicalize()?;
    let child = child_fixture(&path, "managed")?;
    wait_for_file(&path.join("entered")).await?;
    let cancellation = CancellationToken::new();
    assert!(
        tokio::time::timeout(Duration::from_millis(100), acquire(&path, &cancellation))
            .await
            .is_err()
    );
    finish_fixture(child, &path).await?;
    let _held = tokio::time::timeout(FIXTURE_LIMIT, acquire(&path, &cancellation)).await??;
    Ok(())
}

#[tokio::test]
async fn cancellation_before_acquisition_does_not_create_a_lock() -> anyhow::Result<()> {
    let root = private_root()?;
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    assert!(matches!(
        acquire(root.path(), &cancellation).await,
        Err(AdmissionError::Cancelled)
    ));
    assert!(!root.path().join(LOCK_NAME).exists());
    Ok(())
}

#[tokio::test]
async fn cancellation_while_waiting_never_releases_the_current_owner() -> anyhow::Result<()> {
    let root = private_root()?;
    let path = root.path().canonicalize()?;
    let owner_token = CancellationToken::new();
    let held = acquire(&path, &owner_token).await?;
    let cancellation = CancellationToken::new();
    let waiting = acquire(&path, &cancellation);
    tokio::pin!(waiting);
    assert!(
        tokio::time::timeout(Duration::from_millis(100), &mut waiting)
            .await
            .is_err()
    );
    cancellation.cancel();
    let result = tokio::time::timeout(Duration::from_secs(1), &mut waiting).await?;
    assert!(matches!(result, Err(AdmissionError::Cancelled)));
    assert!(
        tokio::time::timeout(Duration::from_millis(100), acquire(&path, &owner_token))
            .await
            .is_err()
    );
    drop(held);
    let _next = acquire(&path, &owner_token).await?;
    Ok(())
}

#[tokio::test]
async fn dropping_waiter_preserves_the_owner_and_its_lock_file() -> anyhow::Result<()> {
    let root = private_root()?;
    let path = root.path().canonicalize()?;
    let cancellation = CancellationToken::new();
    let held = acquire(&path, &cancellation).await?;
    assert!(
        tokio::time::timeout(Duration::from_millis(100), acquire(&path, &cancellation))
            .await
            .is_err()
    );
    assert!(path.join(LOCK_NAME).is_file());
    drop(held);
    let _next = acquire(&path, &cancellation).await?;
    Ok(())
}

#[tokio::test]
async fn a_lock_replaced_while_waiting_is_not_admitted() -> anyhow::Result<()> {
    let root = private_root()?;
    let path = root.path().canonicalize()?;
    let cancellation = CancellationToken::new();
    let held = acquire(&path, &cancellation).await?;
    let waiting = acquire(&path, &cancellation);
    tokio::pin!(waiting);
    assert!(
        tokio::time::timeout(Duration::from_millis(100), &mut waiting)
            .await
            .is_err()
    );
    std::fs::rename(path.join(LOCK_NAME), path.join("old-lock"))?;
    let replacement = std::fs::File::create(path.join(LOCK_NAME))?;
    replacement.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    drop(held);
    let answer = tokio::time::timeout(FIXTURE_LIMIT, &mut waiting).await?;
    assert!(matches!(answer, Err(AdmissionError::UnsafePath { .. })));
    Ok(())
}

#[tokio::test]
async fn acquiring_and_releasing_never_truncates_existing_lock_content() -> anyhow::Result<()> {
    let root = private_root()?;
    let path = root.path().canonicalize()?;
    let lock = path.join(LOCK_NAME);
    std::fs::write(&lock, b"retained")?;
    std::fs::set_permissions(&lock, std::fs::Permissions::from_mode(0o600))?;
    let cancellation = CancellationToken::new();
    drop(acquire(&path, &cancellation).await?);
    assert_eq!(std::fs::read(lock)?, b"retained");
    Ok(())
}

#[tokio::test]
async fn cancellation_does_not_signal_an_unmanaged_fixture_process() -> anyhow::Result<()> {
    let root = private_root()?;
    let path = root.path().canonicalize()?;
    let owner_token = CancellationToken::new();
    let _held = acquire(&path, &owner_token).await?;
    let mut child = child_fixture(&path, "unmanaged")?;
    wait_for_file(&path.join("entered")).await?;
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    assert!(matches!(
        acquire(&path, &cancellation).await,
        Err(AdmissionError::Cancelled)
    ));
    assert!(child.try_wait()?.is_none());
    finish_fixture(child, &path).await?;
    Ok(())
}

#[tokio::test]
async fn absent_relative_and_public_roots_are_refused() -> anyhow::Result<()> {
    let root = private_root()?;
    let cancellation = CancellationToken::new();
    for path in [PathBuf::from("relative-state"), root.path().join("absent")] {
        assert!(acquire(&path, &cancellation).await.is_err());
    }
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o755))?;
    assert!(matches!(
        acquire(root.path(), &cancellation).await,
        Err(AdmissionError::UnsafePath { .. })
    ));
    assert!(!root.path().join(LOCK_NAME).exists());
    Ok(())
}

#[tokio::test]
async fn symlinked_roots_ancestors_and_lock_files_are_refused() -> anyhow::Result<()> {
    let root = private_root()?;
    let target = private_root()?;
    let cancellation = CancellationToken::new();
    let alias = root.path().join("alias");
    std::os::unix::fs::symlink(target.path(), &alias)?;
    std::fs::create_dir(target.path().join("nested"))?;
    for path in [&alias, &alias.join("nested")] {
        assert!(acquire(path, &cancellation).await.is_err());
    }
    let target_file = target.path().join("original");
    std::fs::write(&target_file, b"unchanged")?;
    std::os::unix::fs::symlink(&target_file, root.path().join(LOCK_NAME))?;
    assert!(acquire(root.path(), &cancellation).await.is_err());
    assert_eq!(std::fs::read(target_file)?, b"unchanged");
    Ok(())
}

#[tokio::test]
async fn directory_hardlinked_and_public_lock_files_are_refused() -> anyhow::Result<()> {
    let cancellation = CancellationToken::new();
    let directory_root = private_root()?;
    std::fs::create_dir(directory_root.path().join(LOCK_NAME))?;
    assert!(acquire(directory_root.path(), &cancellation).await.is_err());
    let linked_root = private_root()?;
    let original = linked_root.path().join("original");
    std::fs::write(&original, b"unchanged")?;
    std::fs::set_permissions(&original, std::fs::Permissions::from_mode(0o600))?;
    std::fs::hard_link(&original, linked_root.path().join(LOCK_NAME))?;
    assert!(matches!(
        acquire(linked_root.path(), &cancellation).await,
        Err(AdmissionError::UnsafePath { .. })
    ));
    assert_eq!(std::fs::read(original)?, b"unchanged");
    let public_root = private_root()?;
    let public = public_root.path().join(LOCK_NAME);
    std::fs::write(&public, b"unchanged")?;
    std::fs::set_permissions(&public, std::fs::Permissions::from_mode(0o644))?;
    assert!(matches!(
        acquire(public_root.path(), &cancellation).await,
        Err(AdmissionError::UnsafePath { .. })
    ));
    assert_eq!(std::fs::read(public)?, b"unchanged");
    Ok(())
}

#[tokio::test]
async fn fifo_lock_is_refused_without_blocking_or_writing() -> anyhow::Result<()> {
    use std::os::unix::ffi::OsStrExt;

    let root = private_root()?;
    let lock = root.path().join(LOCK_NAME);
    let name = std::ffi::CString::new(lock.as_os_str().as_bytes())?;
    let result = unsafe { libc::mkfifo(name.as_ptr(), 0o600) };
    anyhow::ensure!(result == 0, "mkfifo: {}", std::io::Error::last_os_error());
    let cancellation = CancellationToken::new();
    let answer =
        tokio::time::timeout(Duration::from_secs(1), acquire(root.path(), &cancellation)).await?;
    assert!(matches!(answer, Err(AdmissionError::UnsafePath { .. })));
    Ok(())
}

#[test]
fn unsupported_lock_error_is_explicit() {
    let error = super::io_error(
        "test",
        Path::new("/test"),
        std::io::ErrorKind::Unsupported.into(),
    );
    assert!(matches!(error, AdmissionError::Unsupported));
}
