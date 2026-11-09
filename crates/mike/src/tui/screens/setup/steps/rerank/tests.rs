use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crossterm::event::KeyCode;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::Rerank;
use super::check::{MODEL_NAME, check_call};
use super::install::{LLAMA_VERSION, MODEL_FILE, Paths, Sources, render_units};
use super::plan::{Current, RerankAction, RerankInputs, plan_panel};
use crate::db_provision::{Call, Captured, Runner};
use crate::tui::client::ApiClient;
use crate::tui::screens::setup::harness::{Harness, key, render_step, typed};
use crate::tui::screens::setup::step::{Outcome, WizardStep};

#[derive(Default)]
struct Fake {
    gpu: Option<&'static str>,
    systemd: bool,
    calls: Mutex<Vec<String>>,
}

fn answer(success: bool, stdout: &str) -> io::Result<Captured> {
    Ok(Captured {
        success,
        stdout: stdout.to_string(),
        stderr: if success {
            String::new()
        } else {
            "no".to_string()
        },
    })
}

impl Runner for Fake {
    async fn run(&self, call: &Call) -> io::Result<Captured> {
        if let Ok(mut calls) = self.calls.lock() {
            calls.push(call.shown());
        }
        match call.program.as_str() {
            "nvidia-smi" => answer(self.gpu.is_some(), self.gpu.unwrap_or_default()),
            "systemctl" => answer(self.systemd && call.args[1] != "is-active", ""),
            "tar" => {
                let status = std::process::Command::new("tar")
                    .args(&call.args)
                    .status()?;
                answer(status.success(), "")
            }
            other => Err(io::Error::other(format!("unexpected program {other}"))),
        }
    }
}

impl Fake {
    fn host() -> Self {
        Self {
            gpu: Some("GPU 0: NVIDIA GeForce RTX 2080 SUPER (UUID: x)"),
            systemd: true,
            calls: Mutex::default(),
        }
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().map(|c| c.clone()).unwrap_or_default()
    }
}

fn host_inputs() -> RerankInputs {
    RerankInputs {
        gpu: Some("GPU 0: NVIDIA GeForce RTX 2080 SUPER".to_string()),
        systemd: true,
        proxyd: Some(PathBuf::from("/usr/lib/systemd/systemd-socket-proxyd")),
        current: Some(Current {
            enabled: false,
            url: String::new(),
        }),
        ..RerankInputs::default()
    }
}

fn actions(inputs: &RerankInputs) -> Vec<RerankAction> {
    plan_panel(inputs)
        .choices
        .iter()
        .map(|c| c.action)
        .collect()
}

#[test]
fn a_machine_with_a_card_and_systemd_is_offered_to_host_the_reranker() {
    let inputs = host_inputs();
    assert_eq!(
        actions(&inputs),
        vec![
            RerankAction::InstallHere,
            RerankAction::OtherMachine,
            RerankAction::Leave
        ]
    );
    let panel = plan_panel(&inputs);
    assert!(panel.choices[0].recommended);
    assert_eq!(
        panel.choices[0].commands,
        vec!["systemctl --user enable --now mike-stack-rerank.socket"]
    );
    let found = panel.found.join("\n");
    assert!(found.contains("RTX 2080 SUPER"), "{found}");
    assert!(
        found.contains(&format!(
            "llama-server {LLAMA_VERSION} is not installed yet"
        )),
        "{found}"
    );
    assert!(found.contains("Reranking is off"), "{found}");
}

#[test]
fn without_a_card_systemd_or_the_forwarder_another_machine_is_recommended() {
    let mut no_gpu = host_inputs();
    no_gpu.gpu = None;
    let mut no_systemd = host_inputs();
    no_systemd.systemd = false;
    let mut no_proxyd = host_inputs();
    no_proxyd.proxyd = None;
    for (inputs, says) in [
        (no_gpu, "No NVIDIA graphics card"),
        (no_systemd, "No systemd user manager"),
        (no_proxyd, "systemd-socket-proxyd is not installed"),
    ] {
        assert_eq!(
            actions(&inputs),
            vec![RerankAction::OtherMachine, RerankAction::Leave]
        );
        let panel = plan_panel(&inputs);
        assert!(panel.choices[0].recommended);
        assert!(panel.found.join("\n").contains(says), "{says}");
    }
}

#[test]
fn another_machine_takes_an_address_prefilled_with_the_current_one() {
    let mut inputs = host_inputs();
    inputs.current = Some(Current {
        enabled: true,
        url: "http://omit:11435".to_string(),
    });
    let panel = plan_panel(&inputs);
    let other = panel
        .choices
        .iter()
        .find(|c| c.action == RerankAction::OtherMachine);
    assert_eq!(
        other
            .and_then(|c| c.input.as_ref())
            .map(|i| i.value.clone()),
        Some("http://omit:11435".to_string())
    );
    assert!(
        panel
            .found
            .join("\n")
            .contains("Reranking is on: http://omit:11435")
    );
}

#[test]
fn the_units_name_the_installed_server_model_and_forwarder() {
    let paths = Paths::for_home(Path::new("/home/u"), None);
    let units = render_units(&paths, Path::new("/usr/lib/systemd/systemd-socket-proxyd"));
    let names: Vec<&str> = units.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        names,
        vec![
            "mike-stack-rerank.socket",
            "mike-stack-rerank-proxy.service",
            "mike-stack-rerank.service"
        ]
    );
    assert!(units[0].1.contains("ListenStream=0.0.0.0:11435"));
    assert!(
        units[0]
            .1
            .contains("Service=mike-stack-rerank-proxy.service")
    );
    assert!(units[1].1.contains(
        "ExecStart=/usr/lib/systemd/systemd-socket-proxyd --exit-idle-time=600 127.0.0.1:11436"
    ));
    assert!(units[1].1.contains("Requires=mike-stack-rerank.service"));
    assert!(units[2].1.contains("StopWhenUnneeded=yes"));
    assert!(units[2].1.contains(&format!(
        "ExecStart=/home/u/.local/share/mike/llama.cpp/{LLAMA_VERSION}/llama-server --reranking --pooling rank -m /home/u/.local/share/mike/models/{MODEL_FILE}"
    )));
    assert!(units.iter().all(|(_, text)| !text.contains('@')));
    let xdg = Paths::for_home(Path::new("/home/u"), Some(Path::new("/cfg")));
    assert_eq!(xdg.units, PathBuf::from("/cfg/systemd/user"));
}

async fn reranker(order: &[(usize, f64)]) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/rerank"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": order.iter().map(|(i, s)| json!({"index": i, "relevance_score": s})).collect::<Vec<_>>()
        })))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn the_check_passes_only_when_the_answer_comes_first() {
    let right = reranker(&[(1, 4.2), (0, -3.0), (2, -5.0)]).await;
    let finding = check_call(&format!("{}/", right.uri())).await;
    assert!(finding.passed(), "{}", finding.text);
    assert!(
        finding.text.contains("Reranked 3 passages"),
        "{}",
        finding.text
    );
    let wrong = reranker(&[(0, 4.2), (1, -3.0), (2, -5.0)]).await;
    let finding = check_call(&wrong.uri()).await;
    assert!(!finding.passed());
    assert!(finding.text.contains("wrong one first"), "{}", finding.text);
    let short = reranker(&[(1, 4.2)]).await;
    assert!(
        check_call(&short.uri())
            .await
            .text
            .contains("returned 1 scores")
    );
    let down = check_call("http://127.0.0.1:1").await;
    assert!(!down.passed());
    assert!(
        down.text
            .contains("Reranking through http://127.0.0.1:1 failed")
    );
}

async fn proxy_server() -> MockServer {
    let server = MockServer::start().await;
    let get = |p: &str, body: Value| {
        Mock::given(method("GET"))
            .and(path(p.to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
    };
    get("/api/health", json!({"status": "ok"}))
        .mount(&server)
        .await;
    get(
        "/api/settings",
        json!({"settings": [
            {"key": "rerank.enabled", "value": false},
            {"key": "rerank.url", "value": ""}
        ]}),
    )
    .mount(&server)
    .await;
    Mock::given(method("PATCH"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;
    server
}

async fn patched(server: &MockServer) -> Vec<(String, String)> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|r| r.method.as_str() == "PATCH")
        .map(|r| {
            (
                r.url.path().to_string(),
                String::from_utf8_lossy(&r.body).to_string(),
            )
        })
        .collect()
}

fn harness_for(proxy: &MockServer) -> anyhow::Result<Harness> {
    let mut harness = Harness::new()?;
    harness.config.inspect_listen_addr = proxy.address().to_string();
    harness.client = ApiClient::new(&harness.config);
    Ok(harness)
}

fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn llama_archive(dir: &Path) -> anyhow::Result<Vec<u8>> {
    let tree = dir.join("tree");
    let top = tree.join(format!("llama-{LLAMA_VERSION}"));
    std::fs::create_dir_all(&top)?;
    std::fs::write(top.join("llama-server"), "#!/bin/sh\n")?;
    let archive = dir.join("llama.tgz");
    let status = std::process::Command::new("tar")
        .arg("czf")
        .arg(&archive)
        .arg("-C")
        .arg(&tree)
        .arg(format!("llama-{LLAMA_VERSION}"))
        .status()?;
    anyhow::ensure!(status.success(), "tar failed");
    Ok(std::fs::read(archive)?)
}

async fn downloads(archive: Vec<u8>) -> (MockServer, Sources) {
    let server = MockServer::start().await;
    let model = b"gguf".to_vec();
    let sources = Sources {
        llama_url: format!("{}/llama.tgz", server.uri()),
        llama_sha256: sha(&archive),
        model_url: format!("{}/model.gguf", server.uri()),
        model_sha256: sha(&model),
    };
    for (p, body) in [("/llama.tgz", archive), ("/model.gguf", model)] {
        Mock::given(method("GET"))
            .and(path(p))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
            .mount(&server)
            .await;
    }
    (server, sources)
}

#[tokio::test]
async fn running_it_here_installs_both_files_writes_the_units_starts_the_socket_and_sets_three_settings()
-> anyhow::Result<()> {
    let proxy = proxy_server().await;
    let mut harness = harness_for(&proxy)?;
    let (_files, sources) = downloads(llama_archive(harness.dir.path())?).await;
    let ranked = reranker(&[(1, 4.2), (0, -3.0), (2, -5.0)]).await;
    let proxyd = harness.add_executable("systemd-socket-proxyd", "#!/bin/sh\n")?;
    let mut step = Rerank::with_parts(Fake::host(), sources, &ranked.uri(), vec![proxyd]);
    step.enter(&mut harness.cx()).await;
    assert!(render_step(&step).contains("Run the reranker on this machine (recommended)"));
    step.key(key(KeyCode::Enter), &mut harness.cx()).await;
    let text = render_step(&step);
    assert!(
        text.contains(&format!("Installed llama-server {LLAMA_VERSION}")),
        "{text}"
    );
    assert!(text.contains("Wrote 3 reranker units"), "{text}");
    assert!(text.contains("accepted 3 rerank settings"), "{text}");
    let home = harness.home();
    let paths = Paths::for_home(&home, None);
    assert!(paths.server().is_file());
    assert_eq!(std::fs::read(paths.model())?, b"gguf");
    assert!(paths.units.join("mike-stack-rerank.service").is_file());
    assert!(
        step.runner
            .calls()
            .contains(&"systemctl --user enable --now mike-stack-rerank.socket".to_string())
    );
    let writes = patched(&proxy).await;
    for (setting, value) in [
        ("rerank.url", ranked.uri()),
        ("rerank.model", MODEL_NAME.to_string()),
        ("rerank.enabled", "true".to_string()),
    ] {
        assert!(
            writes
                .iter()
                .any(|(p, b)| p.ends_with(setting) && b.contains(&value)),
            "{writes:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_download_with_the_wrong_digest_stops_before_any_unit_is_written() -> anyhow::Result<()> {
    let proxy = proxy_server().await;
    let mut harness = harness_for(&proxy)?;
    let (_files, mut sources) = downloads(llama_archive(harness.dir.path())?).await;
    sources.llama_sha256 = "0".repeat(64);
    let proxyd = harness.add_executable("systemd-socket-proxyd", "#!/bin/sh\n")?;
    let mut step = Rerank::with_parts(Fake::host(), sources, "http://127.0.0.1:1", vec![proxyd]);
    step.enter(&mut harness.cx()).await;
    step.key(key(KeyCode::Enter), &mut harness.cx()).await;
    let text = render_step(&step);
    assert!(text.contains("Installing llama-server failed"), "{text}");
    let paths = Paths::for_home(&harness.home(), None);
    assert!(!paths.units.join("mike-stack-rerank.socket").exists());
    assert!(!step.runner.calls().iter().any(|c| c.contains("enable")));
    assert!(patched(&proxy).await.is_empty());
    Ok(())
}

#[tokio::test]
async fn another_machine_is_checked_before_its_address_is_set() -> anyhow::Result<()> {
    let proxy = proxy_server().await;
    let mut harness = harness_for(&proxy)?;
    let ranked = reranker(&[(1, 4.2), (0, -3.0), (2, -5.0)]).await;
    let mut step = Rerank::with_parts(
        Fake::default(),
        Sources::pinned(),
        "http://127.0.0.1:1",
        vec![],
    );
    step.enter(&mut harness.cx()).await;
    step.key(key(KeyCode::Char('e')), &mut harness.cx()).await;
    for event in typed(&ranked.uri()) {
        step.key(event, &mut harness.cx()).await;
    }
    step.key(key(KeyCode::Enter), &mut harness.cx()).await;
    step.key(key(KeyCode::Enter), &mut harness.cx()).await;
    let writes = patched(&proxy).await;
    assert!(
        writes
            .iter()
            .any(|(p, b)| p.ends_with("rerank.url") && b.contains(&ranked.uri())),
        "{writes:?}"
    );
    let mut dead = Rerank::with_parts(
        Fake::default(),
        Sources::pinned(),
        "http://127.0.0.1:1",
        vec![],
    );
    let proxy = proxy_server().await;
    let mut harness = harness_for(&proxy)?;
    dead.enter(&mut harness.cx()).await;
    dead.key(key(KeyCode::Char('e')), &mut harness.cx()).await;
    for event in typed("http://127.0.0.1:1") {
        dead.key(event, &mut harness.cx()).await;
    }
    dead.key(key(KeyCode::Enter), &mut harness.cx()).await;
    dead.key(key(KeyCode::Enter), &mut harness.cx()).await;
    assert!(render_step(&dead).contains("failed"));
    assert!(patched(&proxy).await.is_empty());
    Ok(())
}

#[tokio::test]
async fn leaving_it_off_changes_nothing_and_continues() -> anyhow::Result<()> {
    let proxy = proxy_server().await;
    let mut harness = harness_for(&proxy)?;
    let mut step = Rerank::with_parts(
        Fake::default(),
        Sources::pinned(),
        "http://127.0.0.1:1",
        vec![],
    );
    step.enter(&mut harness.cx()).await;
    step.key(key(KeyCode::Char('j')), &mut harness.cx()).await;
    step.key(key(KeyCode::Enter), &mut harness.cx()).await;
    assert!(patched(&proxy).await.is_empty());
    assert_eq!(
        step.key(key(KeyCode::Enter), &mut harness.cx()).await,
        Outcome::Next
    );
    Ok(())
}
