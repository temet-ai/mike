use std::io::Write;
use std::path::Path;
use std::time::Duration;

use tokio::io::AsyncReadExt;

pub(super) struct Capture {
    pub bytes: Vec<u8>,
    log: Option<std::fs::File>,
}

impl Capture {
    pub(super) fn new(log: Option<&Path>) -> Self {
        let log = log.and_then(|path| match std::fs::File::create(path) {
            Ok(file) => Some(file),
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "fleet: managed output log could not be opened");
                None
            }
        });
        Self {
            bytes: Vec::new(),
            log,
        }
    }

    pub(super) fn append(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
        if let Some(file) = &mut self.log
            && let Err(error) = file.write_all(bytes)
        {
            tracing::warn!(%error, "fleet: managed output log append failed");
            self.log = None;
        }
    }

    pub(super) fn diagnostic(&mut self, text: &str) {
        if !self.bytes.is_empty() && !self.bytes.ends_with(b"\n") {
            self.append(b"\n");
        }
        self.append(text.as_bytes());
    }

    pub(super) async fn drain(&mut self, output: &mut tokio::process::ChildStdout) -> bool {
        let mut buffer = [0u8; 16 * 1024];
        let drain = async {
            loop {
                match output.read(&mut buffer).await {
                    Ok(0) => return true,
                    Ok(count) => self.append(&buffer[..count]),
                    Err(error) => {
                        self.diagnostic(&format!("managed output read failed: {error}"));
                        return false;
                    }
                }
            }
        };
        match tokio::time::timeout(Duration::from_secs(1), drain).await {
            Ok(complete) => complete,
            Err(_) => {
                self.diagnostic("managed output drain timed out; an escaped descendant may still hold the pipe; captured bytes are retained");
                false
            }
        }
    }

    pub(super) fn text(self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
}
