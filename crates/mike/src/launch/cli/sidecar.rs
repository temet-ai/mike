use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(crate) const SIDECAR_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) async fn sidecar_reachable(url: &str) -> Result<u16, String> {
    let client = reqwest::Client::builder()
        .timeout(SIDECAR_PROBE_TIMEOUT)
        .no_proxy()
        .build()
        .map_err(|e| format!("building the probe client: {e}"))?;
    let target = format!("{}/", url.trim_end_matches('/'));
    client
        .get(&target)
        .send()
        .await
        .map(|resp| resp.status().as_u16())
        .map_err(|e| format!("GET {target}: {e}"))
}

pub(crate) async fn inspect_api_reachable(inspect_addr: &str) -> bool {
    crate::stack::probe_health(inspect_addr).await
}

pub(crate) fn locate_executable(bin: &str, path: Option<&OsStr>) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let executable = |p: &Path| {
        std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    };
    if bin.contains('/') {
        let p = PathBuf::from(bin);
        return executable(&p).then_some(p);
    }
    std::env::split_paths(path?)
        .map(|dir| dir.join(bin))
        .find(|p| executable(p))
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sidecar_reachable_refused_port_is_err() {
        let err = sidecar_reachable("http://127.0.0.1:1").await.unwrap_err();
        assert!(err.contains("GET http://127.0.0.1:1/"), "{err}");
    }

    #[tokio::test]
    async fn sidecar_reachable_answering_server_is_ok_with_its_status() -> anyhow::Result<()> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let server = tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = [0u8; 1024];
                let _ = socket.read(&mut buf).await;
                let _ = socket
                    .write_all(b"HTTP/1.1 400 Bad Request\r\ncontent-length: 0\r\n\r\n")
                    .await;
            }
        });
        let status = sidecar_reachable(&format!("http://{addr}/")).await;
        server.abort();
        assert_eq!(status, Ok(400));
        Ok(())
    }

    #[test]
    fn locate_executable_finds_sh_on_a_path_and_nothing_off_it() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let found = locate_executable("sh", Some(OsStr::new("/usr/bin:/bin")));
        assert!(found.is_some_and(|p| p.ends_with("bin/sh")));
        assert_eq!(locate_executable("sh", Some(dir.path().as_os_str())), None);
        assert_eq!(locate_executable("sh", None), None);
        Ok(())
    }

    #[test]
    fn locate_executable_with_a_slash_checks_that_path_only() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let plain = dir.path().join("notexec");
        std::fs::write(&plain, "x")?;
        assert_eq!(locate_executable(&plain.to_string_lossy(), None), None);
        assert_eq!(
            locate_executable("/bin/sh", None),
            Some(PathBuf::from("/bin/sh"))
        );
        Ok(())
    }
}
