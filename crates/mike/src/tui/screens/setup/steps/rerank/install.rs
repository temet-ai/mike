use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::db_provision::{Call, Runner};
use crate::finding::Finding;

pub(super) const LLAMA_VERSION: &str = "b11895";
pub(super) const MODEL_FILE: &str = "bge-reranker-v2-m3-Q8_0.gguf";
pub(super) const SOCKET_UNIT: &str = "mike-stack-rerank.socket";
const DOWNLOAD_WAIT: Duration = Duration::from_secs(1800);
const SOCKET_TEMPLATE: &str =
    include_str!("../../../../../../../../scripts/systemd/mike-stack-rerank.socket.in");
const PROXY_TEMPLATE: &str =
    include_str!("../../../../../../../../scripts/systemd/mike-stack-rerank-proxy.service.in");
const SERVER_TEMPLATE: &str =
    include_str!("../../../../../../../../scripts/systemd/mike-stack-rerank.service.in");

#[derive(Debug, Clone)]
pub(super) struct Sources {
    pub(super) llama_url: String,
    pub(super) llama_sha256: String,
    pub(super) model_url: String,
    pub(super) model_sha256: String,
}

impl Sources {
    pub(super) fn pinned() -> Self {
        Self {
            llama_url: format!(
                "https://github.com/ggml-org/llama.cpp/releases/download/{LLAMA_VERSION}/llama-{LLAMA_VERSION}-bin-ubuntu-vulkan-x64.tar.gz"
            ),
            llama_sha256: "c01f0ef0a1c2c8ad79595feb4aeb96b1813b0683a23699daa4114c783b12e6a9"
                .to_string(),
            model_url: format!(
                "https://huggingface.co/gpustack/bge-reranker-v2-m3-GGUF/resolve/main/{MODEL_FILE}"
            ),
            model_sha256: "a43c7c9b11a4c1517e5bf95151960e1621d1b72f7a493364b01e386cf1aaa1d3"
                .to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct Paths {
    pub(super) data: PathBuf,
    pub(super) units: PathBuf,
}

impl Paths {
    pub(super) fn for_home(home: &Path, xdg_config_home: Option<&Path>) -> Self {
        let config = xdg_config_home.map_or_else(|| home.join(".config"), Path::to_path_buf);
        Self {
            data: home.join(".local/share/mike"),
            units: config.join("systemd/user"),
        }
    }

    pub(super) fn llama_dir(&self) -> PathBuf {
        self.data.join("llama.cpp").join(LLAMA_VERSION)
    }

    pub(super) fn server(&self) -> PathBuf {
        self.llama_dir().join("llama-server")
    }

    pub(super) fn model(&self) -> PathBuf {
        self.data.join("models").join(MODEL_FILE)
    }
}

pub(super) async fn download(url: &str, sha256: &str, dest: &Path) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(DOWNLOAD_WAIT)
        .build()
        .map_err(|e| e.to_string())?;
    let mut response = client
        .get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| e.to_string())?;
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| e.to_string())?;
    }
    let partial = dest.with_extension("partial");
    let mut file = tokio::fs::File::create(&partial)
        .await
        .map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        hasher.update(&chunk);
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
    }
    file.flush().await.map_err(|e| e.to_string())?;
    let got = hex::encode(hasher.finalize());
    if got != sha256 {
        let _ = tokio::fs::remove_file(&partial).await;
        return Err(format!("the file's SHA-256 is {got}, expected {sha256}"));
    }
    tokio::fs::rename(&partial, dest)
        .await
        .map_err(|e| e.to_string())
}

async fn unpack<R: Runner>(runner: &R, archive: &Path, paths: &Paths) -> Result<(), String> {
    let staging = paths.data.join("llama.cpp").join(".unpack");
    let _ = tokio::fs::remove_dir_all(&staging).await;
    tokio::fs::create_dir_all(&staging)
        .await
        .map_err(|e| e.to_string())?;
    let archive_text = archive.display().to_string();
    let staging_text = staging.display().to_string();
    let call = Call::new("tar", &["xzf", &archive_text, "-C", &staging_text]);
    match runner.run(&call).await {
        Ok(out) if out.success => {}
        Ok(out) => return Err(format!("`{}` failed: {}", call.shown(), out.detail())),
        Err(e) => return Err(format!("`{}` could not run: {e}", call.shown())),
    }
    let unpacked = staging.join(format!("llama-{LLAMA_VERSION}"));
    let _ = tokio::fs::remove_dir_all(paths.llama_dir()).await;
    tokio::fs::rename(&unpacked, paths.llama_dir())
        .await
        .map_err(|e| format!("the archive did not hold llama-{LLAMA_VERSION}/: {e}"))?;
    let _ = tokio::fs::remove_dir_all(&staging).await;
    let _ = tokio::fs::remove_file(archive).await;
    Ok(())
}

pub(super) async fn ensure_llama<R: Runner>(
    runner: &R,
    sources: &Sources,
    paths: &Paths,
) -> Finding {
    if paths.server().is_file() {
        return Finding::ok(format!(
            "llama-server {LLAMA_VERSION} is already in {}.",
            paths.llama_dir().display()
        ));
    }
    let archive = paths
        .data
        .join("llama.cpp")
        .join(format!("llama-{LLAMA_VERSION}.tar.gz"));
    let result = match download(&sources.llama_url, &sources.llama_sha256, &archive).await {
        Ok(()) => unpack(runner, &archive, paths).await,
        Err(why) => Err(format!("downloading {} failed: {why}", sources.llama_url)),
    };
    match result {
        Ok(()) if paths.server().is_file() => Finding::ok(format!(
            "Installed llama-server {LLAMA_VERSION} into {}.",
            paths.llama_dir().display()
        )),
        Ok(()) => Finding::failed(format!(
            "The llama.cpp {LLAMA_VERSION} archive has no llama-server."
        )),
        Err(why) => Finding::failed(format!("Installing llama-server failed: {why}")),
    }
}

pub(super) async fn ensure_model(sources: &Sources, paths: &Paths) -> Finding {
    let model = paths.model();
    if model.is_file() {
        return Finding::ok(format!("{MODEL_FILE} is already downloaded."));
    }
    match download(&sources.model_url, &sources.model_sha256, &model).await {
        Ok(()) => Finding::ok(format!("Downloaded {MODEL_FILE} into {}.", model.display())),
        Err(why) => Finding::failed(format!("Downloading {MODEL_FILE} failed: {why}")),
    }
}

pub(super) fn render_units(paths: &Paths, proxyd: &Path) -> Vec<(&'static str, String)> {
    let fill = |template: &str| {
        template
            .replace("@LLAMA_DIR@", &paths.llama_dir().display().to_string())
            .replace("@MODEL@", &paths.model().display().to_string())
            .replace("@SOCKET_PROXYD@", &proxyd.display().to_string())
    };
    vec![
        (SOCKET_UNIT, fill(SOCKET_TEMPLATE)),
        ("mike-stack-rerank-proxy.service", fill(PROXY_TEMPLATE)),
        ("mike-stack-rerank.service", fill(SERVER_TEMPLATE)),
    ]
}

pub(super) fn write_units(paths: &Paths, units: &[(&'static str, String)]) -> Finding {
    if let Err(e) = std::fs::create_dir_all(&paths.units) {
        return Finding::failed(format!("Could not create {}: {e}", paths.units.display()));
    }
    for (name, text) in units {
        let path = paths.units.join(name);
        if let Err(e) = std::fs::write(&path, text) {
            return Finding::failed(format!("Could not write {}: {e}", path.display()));
        }
    }
    Finding::ok(format!(
        "Wrote {} reranker units into {}.",
        units.len(),
        paths.units.display()
    ))
}

pub(super) async fn enable<R: Runner>(runner: &R) -> Finding {
    let calls = [
        Call::new("systemctl", &["--user", "daemon-reload"]),
        Call::new("systemctl", &["--user", "enable", "--now", SOCKET_UNIT]),
    ];
    for call in &calls {
        match runner.run(call).await {
            Ok(out) if out.success => {}
            Ok(out) => {
                return Finding::failed(format!("`{}` failed: {}", call.shown(), out.detail()));
            }
            Err(e) => return Finding::failed(format!("`{}` could not run: {e}", call.shown())),
        }
    }
    Finding::ok(format!(
        "{SOCKET_UNIT} listens on port 11435; the reranker starts on the first request and stops after 10 idle minutes."
    ))
}
