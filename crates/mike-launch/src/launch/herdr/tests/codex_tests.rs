use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::{Value, json};
use uuid::Uuid;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{FakeHerdr, agent_entry, envelope, pane_info, process_info};
use crate::launch::herdr::HerdrBackend;
use crate::launch::{CloseOutcome, OpenRequest, Placement, TerminalBackend};

const SESSION: &str = "11111111-2222-4333-8444-555555555555";
const NATIVE: &str = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
const PANE: &str = "w1:p1";

fn request() -> OpenRequest {
    OpenRequest {
        folder: "/tmp/project".into(),
        placement: Placement::Tab,
        env: BTreeMap::from([("MIKE_SESSION_ID".into(), SESSION.into())]),
        title: "Codex".into(),
        run_tab: None,
        run: None,
        program: "codex".into(),
        args: vec![
            "--no-daemon".into(),
            "--".into(),
            "inspect this project".into(),
        ],
        name: "codex-11111111".into(),
        prompt: None,
        prompt_wait_ms: 100,
    }
}

fn listing(thread: Option<&str>) -> String {
    envelope(json!({"agents": [agent_entry(PANE, "codex", thread, "idle")]}))
}

fn startup(fake: &FakeHerdr) {
    fake.answer("pane list", &envelope(json!({"panes": []})))
        .answer(
            "workspace create",
            &envelope(json!({
                "root_pane": pane_info("w1", "w1:t1", PANE, "/tmp/project")
            })),
        )
        .answer(
            "pane process-info",
            &envelope(process_info(PANE, 100, 200, None)),
        );
}

fn backend(fake: &FakeHerdr, server: &MockServer) -> HerdrBackend {
    let mut backend = HerdrBackend::with_binary(fake.bin());
    backend.set_inspect_addr(&server.uri());
    backend
}

async fn binding(server: &MockServer, thread: &str, session: &str) {
    Mock::given(method("GET"))
        .and(path("/api/codex/session"))
        .and(query_param("thread_id", thread))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"session_id": session})))
        .mount(server)
        .await;
}

#[tokio::test]
async fn open_codex_uses_requested_kind_arguments_and_reserved_mike_identity() -> anyhow::Result<()>
{
    let fake = FakeHerdr::new();
    startup(&fake);
    fake.answer(
        "agent start",
        &envelope(json!({
            "agent": agent_entry(PANE, "codex", Some(NATIVE), "working")
        })),
    );
    let req = request();
    let surface = HerdrBackend::with_binary(fake.bin()).open(&req).await?;
    assert_eq!(surface.session_uuid, Some(SESSION.parse()?));
    assert_eq!(surface.pid, Some(200));
    let calls = fake.calls();
    let start = calls
        .iter()
        .find(|call| call[..2] == ["agent", "start"])
        .ok_or_else(|| anyhow::anyhow!("agent start was not called"))?;
    assert_eq!(
        &start[..10],
        [
            "agent",
            "start",
            &req.name,
            "--kind",
            "codex",
            "--pane",
            PANE,
            "--timeout",
            "60000",
            "--"
        ]
    );
    assert_eq!(start[10..], req.args);
    assert!(!calls.iter().any(|call| call[..2] == ["agent", "prompt"]));
    Ok(())
}

#[tokio::test]
async fn open_codex_not_ready_keeps_reserved_identity_without_native_thread() -> anyhow::Result<()>
{
    let fake = FakeHerdr::new();
    startup(&fake);
    fake.error("agent start", "agent_not_ready", "project trust is pending")
        .answer(
            "agent get",
            &envelope(json!({
                "agent": agent_entry(PANE, "codex", None, "blocked")
            })),
        );
    let surface = HerdrBackend::with_binary(fake.bin())
        .open(&request())
        .await?;
    assert_eq!(surface.session_uuid, Some(SESSION.parse()?));
    assert_eq!(surface.agent_state.as_deref(), Some("blocked"));
    Ok(())
}

#[tokio::test]
async fn open_codex_timeout_recovers_only_the_requested_runtime() -> anyhow::Result<()> {
    for runtime in ["codex", "claude"] {
        let fake = FakeHerdr::new();
        startup(&fake);
        fake.error(
            "agent start",
            "timeout",
            "waiting for interactive readiness",
        )
        .answer(
            "agent get",
            &envelope(json!({
                "agent": agent_entry(PANE, runtime, Some(NATIVE), "working")
            })),
        );
        let result = HerdrBackend::with_binary(fake.bin()).open(&request()).await;
        if runtime == "codex" {
            assert_eq!(result?.session_uuid, Some(SESSION.parse()?));
        } else {
            assert!(result.is_err());
        }
    }
    Ok(())
}

#[tokio::test]
async fn list_codex_resolves_uuid_shaped_and_opaque_native_ids() -> anyhow::Result<()> {
    for thread in [NATIVE, " opaque:thread/id "] {
        let server = MockServer::start().await;
        binding(&server, thread, SESSION).await;
        let fake = FakeHerdr::new();
        fake.answer("agent list", &listing(Some(thread)));
        let listed = backend(&fake, &server).list().await?;
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].session_uuid, Some(SESSION.parse()?));
    }
    Ok(())
}

#[tokio::test]
async fn lookup_unresolved_or_missing_native_thread_does_not_invent_mike_identity()
-> anyhow::Result<()> {
    let server = MockServer::start().await;
    for thread in [Some(NATIVE), None] {
        let fake = FakeHerdr::new();
        fake.answer("agent list", &listing(thread));
        let backend = backend(&fake, &server);
        assert_eq!(backend.list().await?[0].session_uuid, None);
        assert_eq!(backend.find_by_session(NATIVE.parse()?).await?, None);
        assert_eq!(backend.find_by_session(SESSION.parse()?).await?, None);
    }
    Ok(())
}

#[tokio::test]
async fn find_codex_rechecks_native_thread_after_reading_the_process() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    binding(&server, NATIVE, SESSION).await;
    for next_thread in [NATIVE, "different-thread"] {
        let fake = FakeHerdr::new();
        fake.answer("agent list", &listing(Some(NATIVE)))
            .answer("agent list", &listing(Some(next_thread)))
            .answer(
                "pane process-info",
                &envelope(process_info(PANE, 100, 200, None)),
            );
        let found = backend(&fake, &server)
            .find_by_session(SESSION.parse()?)
            .await?;
        assert_eq!(found.is_some(), next_thread == NATIVE);
        if let Some(found) = found {
            assert_eq!(found.pid, Some(200));
            assert_eq!(found.session_uuid, Some(SESSION.parse()?));
        }
    }
    Ok(())
}

#[tokio::test]
async fn close_codex_rechecks_binding_and_refuses_replaced_or_unresolved_threads()
-> anyhow::Result<()> {
    let server = MockServer::start().await;
    binding(&server, NATIVE, SESSION).await;
    binding(
        &server,
        "replacement",
        "ffffffff-1111-4222-8333-444444444444",
    )
    .await;
    for thread in [Some(NATIVE), Some("replacement"), Some("unresolved"), None] {
        let fake = FakeHerdr::new();
        fake.answer("agent list", &listing(thread))
            .answer(
                "pane get",
                &envelope(json!({"pane": pane_info("w1", "w1:t1", PANE, "/tmp/project")})),
            )
            .answer("pane close", &envelope(Value::Null));
        let surface = super::super::surface::surface_from_agent(
            &agent_entry(PANE, "codex", Some(NATIVE), "idle"),
            Some(SESSION.parse()?),
            None,
            "tab",
            false,
        )
        .ok_or_else(|| anyhow::anyhow!("fixture has no pane"))?;
        let outcome = backend(&fake, &server)
            .close(&surface, Duration::ZERO)
            .await?;
        let should_close = thread == Some(NATIVE);
        assert_eq!(matches!(outcome, CloseOutcome::Closed { .. }), should_close);
        assert_eq!(
            fake.calls()
                .iter()
                .any(|call| call[..2] == ["pane", "close"]),
            should_close
        );
    }
    Ok(())
}

#[tokio::test]
async fn codex_resolver_failure_does_not_fall_back_to_native_uuid() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(path("/api/codex/session"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let fake = FakeHerdr::new();
    fake.answer("agent list", &listing(Some(NATIVE)));
    assert_eq!(
        backend(&fake, &server)
            .find_by_session(NATIVE.parse::<Uuid>()?)
            .await?,
        None
    );
    assert_eq!(backend(&fake, &server).list().await?[0].session_uuid, None);
    Ok(())
}

#[tokio::test]
async fn claude_list_and_find_survive_an_unrelated_codex_resolver_failure() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(path("/api/codex/session"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let fake = FakeHerdr::new();
    fake.answer(
        "agent list",
        &envelope(json!({"agents": [
            agent_entry("w1:p2", "codex", Some(NATIVE), "idle"),
            agent_entry(PANE, "claude", Some(SESSION), "idle")
        ]})),
    )
    .answer(
        "pane process-info",
        &envelope(process_info(PANE, 100, 200, Some(200))),
    );
    let backend = backend(&fake, &server);
    let listed = backend.list().await?;
    assert_eq!(listed[0].session_uuid, None);
    assert_eq!(listed[1].session_uuid, Some(SESSION.parse()?));
    let found = backend
        .find_by_session(SESSION.parse()?)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Claude session was hidden by unrelated Codex failure"))?;
    assert_eq!(found.surface_ref, PANE);
    Ok(())
}
