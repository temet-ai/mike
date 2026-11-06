use std::cell::RefCell;
use std::path::Path;
use std::time::Duration;

use super::{
    Fake, endpoints, everything_listens, inspect_port_listens, manager, nothing_listens, specs_in,
};
use crate::service::endpoints::Endpoints;
use crate::service::error::ServiceError;
use crate::service::exec::Context;
use crate::service::files::MARKER;
use crate::service::install::{install, plan};
use crate::service::manager::Manager;
use crate::service::platform::Platform;
use crate::service::spec::ServiceKind;

pub(super) struct World {
    pub(super) home: tempfile::TempDir,
    pub(super) manager: Manager,
    pub(super) endpoints: Endpoints,
    pub(super) lines: RefCell<Vec<String>>,
}

impl World {
    pub(super) fn new(platform: Platform) -> anyhow::Result<Self> {
        let home = tempfile::tempdir()?;
        let manager = manager(platform, home.path());
        Ok(Self {
            home,
            manager,
            endpoints: endpoints(),
            lines: RefCell::new(Vec::new()),
        })
    }

    pub(super) fn path(&self) -> &Path {
        self.home.path()
    }

    pub(super) fn file(&self, kind: ServiceKind) -> std::path::PathBuf {
        self.manager.file_path(kind)
    }

    pub(super) fn said(&self) -> String {
        self.lines.borrow().join("\n")
    }
}

pub(super) fn context<'a>(
    fake: &'a Fake,
    world: &'a World,
    out: &'a dyn Fn(&str),
    listening: fn(&str) -> bool,
) -> Context<'a, Fake> {
    Context {
        runner: fake,
        manager: &world.manager,
        endpoints: &world.endpoints,
        listening,
        pause: Duration::ZERO,
        out,
    }
}

macro_rules! run_install {
    ($world:expr, $fake:expr, $listening:expr) => {{
        let out = |line: &str| $world.lines.borrow_mut().push(line.to_string());
        let ctx = context(&$fake, &$world, &out, $listening);
        install(&ctx, &specs_in($world.path())).await
    }};
}

#[tokio::test]
async fn a_fresh_launchd_install_writes_both_agents_and_bootstraps_proxy_before_sidecar()
-> anyhow::Result<()> {
    let world = World::new(Platform::Launchd)?;
    let fake = Fake::new();
    run_install!(world, fake, nothing_listens)?;

    for kind in ServiceKind::ALL {
        let text = std::fs::read_to_string(world.file(kind))?;
        assert!(text.contains(MARKER), "{text}");
    }
    assert!(world.path().join(".local/state/mike/logs").is_dir());
    let proxy = fake.position_of("launchctl bootstrap gui/501");
    let sidecar = fake.position_of("ai.temet.mike.forward.plist");
    assert!(
        proxy.is_some() && sidecar.is_some() && proxy < sidecar,
        "{:?}",
        fake.seen()
    );
    let said = world.said();
    assert!(
        said.contains("proxy: started under launchd as ai.temet.mike.proxy"),
        "{said}"
    );
    assert!(
        said.contains("sidecar: started under launchd as ai.temet.mike.forward"),
        "{said}"
    );
    Ok(())
}

#[tokio::test]
async fn installing_twice_changes_nothing_the_second_time() -> anyhow::Result<()> {
    let world = World::new(Platform::Launchd)?;
    let fake = Fake::new();
    run_install!(world, fake, nothing_listens)?;
    let bootstraps = fake.count_containing("bootstrap");
    let before = std::fs::read_to_string(world.file(ServiceKind::Proxy))?;

    world.lines.borrow_mut().clear();
    run_install!(world, fake, everything_listens)?;

    assert_eq!(fake.count_containing("bootstrap"), bootstraps);
    assert_eq!(fake.count_containing("bootout"), 0);
    assert_eq!(
        std::fs::read_to_string(world.file(ServiceKind::Proxy))?,
        before
    );
    let said = world.said();
    assert!(said.contains("is already up to date"), "{said}");
    assert!(
        said.contains("proxy: already running under launchd"),
        "{said}"
    );
    Ok(())
}

#[tokio::test]
async fn a_changed_agent_file_is_replaced_and_the_loaded_agent_is_reloaded() -> anyhow::Result<()> {
    let world = World::new(Platform::Launchd)?;
    let fake = Fake::new();
    run_install!(world, fake, nothing_listens)?;
    let proxy_file = world.file(ServiceKind::Proxy);
    std::fs::write(&proxy_file, format!("<!-- {MARKER} -->\nold content\n"))?;

    run_install!(world, fake, everything_listens)?;

    assert!(!std::fs::read_to_string(&proxy_file)?.contains("old content"));
    let seen = fake.seen();
    let out = seen
        .iter()
        .rposition(|c| c.contains("bootout gui/501/ai.temet.mike.proxy"));
    let back = seen
        .iter()
        .rposition(|c| c.contains("bootstrap gui/501") && c.contains("proxy.plist"));
    assert!(out.is_some() && back.is_some() && out < back, "{seen:?}");
    assert!(world.said().contains("updated"), "{}", world.said());
    Ok(())
}

#[tokio::test]
async fn a_fresh_systemd_install_reloads_then_enables_and_starts_each_unit() -> anyhow::Result<()> {
    let world = World::new(Platform::Systemd)?;
    let fake = Fake::new();
    run_install!(world, fake, nothing_listens)?;

    let reload = fake.position_of("daemon-reload");
    let proxy = fake.position_of("enable --now mike.service");
    let sidecar = fake.position_of("enable --now mike-forward.service");
    assert!(reload < proxy && proxy < sidecar, "{:?}", fake.seen());
    assert!(
        world
            .file(ServiceKind::Proxy)
            .ends_with(".config/systemd/user/mike.service")
    );
    assert!(
        world
            .said()
            .contains("started under systemd as mike.service")
    );
    Ok(())
}

#[tokio::test]
async fn an_active_developer_stack_stops_the_install_before_anything_is_written()
-> anyhow::Result<()> {
    let world = World::new(Platform::Systemd)?;
    let fake = Fake::new().with_dev_units(
        "mike-stack-proxy.service loaded active running mike proxy\nmike-stack-forward.service loaded active running sidecar\n",
    );
    let result = run_install!(world, fake, nothing_listens);

    let Err(ServiceError::DevStackActive { units }) = result else {
        anyhow::bail!("expected the developer stack refusal, got {result:?}");
    };
    assert_eq!(
        units,
        "mike-stack-proxy.service, mike-stack-forward.service"
    );
    for kind in ServiceKind::ALL {
        assert!(!world.file(kind).exists());
    }
    assert_eq!(fake.count_containing("enable"), 0);
    Ok(())
}

#[tokio::test]
async fn a_busy_port_stops_the_install_and_names_what_is_listening() -> anyhow::Result<()> {
    let world = World::new(Platform::Launchd)?;
    let fake = Fake::new();
    let result = run_install!(world, fake, inspect_port_listens);

    let Err(ServiceError::PortInUse { what, addr }) = result else {
        anyhow::bail!("expected the busy port refusal, got {result:?}");
    };
    assert_eq!((what, addr.as_str()), ("inspect", "127.0.0.1:9989"));
    assert!(!world.file(ServiceKind::Proxy).exists());
    assert_eq!(fake.count_containing("bootstrap"), 0);
    Ok(())
}

#[tokio::test]
async fn ports_held_by_mikes_own_loaded_services_are_not_a_conflict() -> anyhow::Result<()> {
    let world = World::new(Platform::Launchd)?;
    let fake = Fake::new().with_loaded(&["ai.temet.mike.proxy", "ai.temet.mike.forward"]);
    run_install!(world, fake, everything_listens)?;
    assert!(world.file(ServiceKind::Proxy).exists());
    Ok(())
}

#[tokio::test]
async fn a_file_mike_did_not_write_is_left_alone_and_nothing_else_is_written() -> anyhow::Result<()>
{
    let world = World::new(Platform::Launchd)?;
    let foreign = world.file(ServiceKind::Sidecar);
    std::fs::create_dir_all(foreign.parent().unwrap_or(world.path()))?;
    std::fs::write(&foreign, "<plist>somebody else's</plist>")?;
    let fake = Fake::new();
    let result = run_install!(world, fake, nothing_listens);

    assert!(
        matches!(result, Err(ServiceError::ForeignFile { .. })),
        "{result:?}"
    );
    assert_eq!(
        std::fs::read_to_string(&foreign)?,
        "<plist>somebody else's</plist>"
    );
    assert!(!world.file(ServiceKind::Proxy).exists());
    Ok(())
}

#[tokio::test]
async fn a_failing_bootstrap_is_retried_and_then_reported_with_the_command() -> anyhow::Result<()> {
    let world = World::new(Platform::Launchd)?;
    let fake = Fake::new().failing_on("bootstrap");
    let result = run_install!(world, fake, nothing_listens);

    let Err(ServiceError::CommandFailed { command, detail }) = result else {
        anyhow::bail!("expected a failed command, got {result:?}");
    };
    assert!(
        command.starts_with("launchctl bootstrap gui/501"),
        "{command}"
    );
    assert_eq!(detail, "boom");
    assert_eq!(fake.count_containing("bootstrap"), 4);
    Ok(())
}

#[test]
fn the_dry_run_plan_shows_every_file_and_command_without_touching_the_disk() -> anyhow::Result<()> {
    let world = World::new(Platform::Launchd)?;
    let lines = plan(&world.manager, &specs_in(world.path()));
    let text = lines.join("\n");
    assert!(text.contains("Would write "), "{text}");
    assert!(text.contains("ai.temet.mike.proxy.plist"), "{text}");
    assert!(text.contains("    <string>serve</string>"), "{text}");
    assert!(text.contains("Would then run:"), "{text}");
    assert!(text.contains("    launchctl bootstrap gui/501 "), "{text}");
    assert!(!world.file(ServiceKind::Proxy).exists());
    Ok(())
}
