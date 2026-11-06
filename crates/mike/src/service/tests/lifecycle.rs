use super::install::{World, context};
use super::{Fake, everything_listens, nothing_listens, specs_in};
use crate::service::error::ServiceError;
use crate::service::files::Presence;
use crate::service::guard::active_dev_units;
use crate::service::install::install;
use crate::service::platform::Platform;
use crate::service::restart::{self, restart};
use crate::service::spec::ServiceKind;
use crate::service::state::Activity;
use crate::service::status::collect;
use crate::service::uninstall::{self, uninstall};

async fn installed(platform: Platform) -> anyhow::Result<(World, Fake)> {
    let world = World::new(platform)?;
    let fake = Fake::new();
    let out = |_: &str| {};
    let ctx = context(&fake, &world, &out, nothing_listens);
    install(&ctx, &specs_in(world.path())).await?;
    Ok((world, fake))
}

#[tokio::test]
async fn uninstall_unloads_then_removes_only_the_files_mike_wrote() -> anyhow::Result<()> {
    let (world, fake) = installed(Platform::Launchd).await?;
    let neighbour = world
        .file(ServiceKind::Proxy)
        .with_file_name("com.example.other.plist");
    std::fs::write(&neighbour, "not mike's")?;

    let out = |line: &str| world.lines.borrow_mut().push(line.to_string());
    uninstall(&context(&fake, &world, &out, nothing_listens)).await?;

    for kind in ServiceKind::ALL {
        assert!(!world.file(kind).exists());
    }
    assert_eq!(std::fs::read_to_string(&neighbour)?, "not mike's");
    let booted_out = fake.position_of("bootout gui/501/ai.temet.mike.forward");
    let other = fake.position_of("bootout gui/501/ai.temet.mike.proxy");
    assert!(
        booted_out.is_some() && other.is_some() && booted_out < other,
        "{:?}",
        fake.seen()
    );
    assert!(world.said().contains("proxy: removed "), "{}", world.said());
    Ok(())
}

#[tokio::test]
async fn uninstall_leaves_a_file_it_did_not_write_and_says_so() -> anyhow::Result<()> {
    let (world, fake) = installed(Platform::Launchd).await?;
    let sidecar = world.file(ServiceKind::Sidecar);
    std::fs::write(&sidecar, "<plist>replaced by the user</plist>")?;

    let out = |line: &str| world.lines.borrow_mut().push(line.to_string());
    uninstall(&context(&fake, &world, &out, nothing_listens)).await?;

    assert_eq!(
        std::fs::read_to_string(&sidecar)?,
        "<plist>replaced by the user</plist>"
    );
    assert!(!world.file(ServiceKind::Proxy).exists());
    let said = world.said();
    assert!(said.contains("sidecar: left alone, because"), "{said}");
    assert_eq!(
        fake.count_containing("bootout gui/501/ai.temet.mike.forward"),
        0
    );
    Ok(())
}

#[tokio::test]
async fn uninstall_with_nothing_installed_runs_no_command() -> anyhow::Result<()> {
    let world = World::new(Platform::Systemd)?;
    let fake = Fake::new();
    let out = |line: &str| world.lines.borrow_mut().push(line.to_string());
    uninstall(&context(&fake, &world, &out, nothing_listens)).await?;
    assert!(fake.seen().is_empty(), "{:?}", fake.seen());
    assert!(world.said().contains("proxy: not installed"));
    Ok(())
}

#[tokio::test]
async fn systemd_uninstall_disables_each_unit_and_reloads_the_manager() -> anyhow::Result<()> {
    let (world, fake) = installed(Platform::Systemd).await?;
    let out = |_: &str| {};
    uninstall(&context(&fake, &world, &out, nothing_listens)).await?;

    assert_eq!(fake.count_containing("disable --now mike.service"), 1);
    assert_eq!(
        fake.count_containing("disable --now mike-forward.service"),
        1
    );
    let last_reload = fake
        .seen()
        .iter()
        .rposition(|c| c.contains("daemon-reload"));
    let last_disable = fake.seen().iter().rposition(|c| c.contains("disable"));
    assert!(last_reload > last_disable, "{:?}", fake.seen());
    Ok(())
}

#[tokio::test]
async fn restart_kicks_each_launchd_agent_proxy_first() -> anyhow::Result<()> {
    let (world, fake) = installed(Platform::Launchd).await?;
    let out = |_: &str| {};
    restart(&context(&fake, &world, &out, nothing_listens)).await?;
    let proxy = fake.position_of("kickstart -k gui/501/ai.temet.mike.proxy");
    let sidecar = fake.position_of("kickstart -k gui/501/ai.temet.mike.forward");
    assert!(proxy.is_some() && proxy < sidecar, "{:?}", fake.seen());
    Ok(())
}

#[tokio::test]
async fn restart_asks_systemd_to_restart_each_unit() -> anyhow::Result<()> {
    let (world, fake) = installed(Platform::Systemd).await?;
    let out = |_: &str| {};
    restart(&context(&fake, &world, &out, nothing_listens)).await?;
    assert_eq!(
        fake.count_containing("systemctl --user restart mike.service"),
        1
    );
    assert_eq!(
        fake.count_containing("systemctl --user restart mike-forward.service"),
        1
    );
    Ok(())
}

#[tokio::test]
async fn restart_without_an_install_points_at_the_install_command() -> anyhow::Result<()> {
    let world = World::new(Platform::Launchd)?;
    let fake = Fake::new();
    let out = |_: &str| {};
    let result = restart(&context(&fake, &world, &out, everything_listens)).await;
    let Err(error) = result else {
        anyhow::bail!("restart should fail when nothing is installed");
    };
    assert!(matches!(
        error,
        ServiceError::NotInstalled { name: "proxy" }
    ));
    assert!(error.to_string().contains("mike service install"));
    Ok(())
}

#[tokio::test]
async fn restart_refuses_a_unit_file_mike_did_not_write() -> anyhow::Result<()> {
    let world = World::new(Platform::Systemd)?;
    let path = world.file(ServiceKind::Proxy);
    std::fs::create_dir_all(path.parent().unwrap_or(world.path()))?;
    std::fs::write(&path, "[Unit]\nDescription=mine\n")?;
    let fake = Fake::new();
    let out = |_: &str| {};
    let result = restart(&context(&fake, &world, &out, nothing_listens)).await;
    assert!(matches!(result, Err(ServiceError::ForeignFile { .. })));
    assert!(fake.seen().is_empty());
    Ok(())
}

#[test]
fn the_dry_run_plans_name_the_commands_for_the_platform() -> anyhow::Result<()> {
    let mac = World::new(Platform::Launchd)?;
    let text = uninstall::plan(&mac.manager).join("\n");
    assert!(
        text.contains("launchctl bootout gui/501/ai.temet.mike.proxy"),
        "{text}"
    );
    let linux = World::new(Platform::Systemd)?;
    let text = restart::plan(&linux.manager).join("\n");
    assert!(
        text.contains("systemctl --user restart mike-forward.service"),
        "{text}"
    );
    Ok(())
}

#[tokio::test]
async fn status_reports_missing_then_running_units_with_their_pids() -> anyhow::Result<()> {
    let world = World::new(Platform::Launchd)?;
    let fake = Fake::new();
    let before = collect(&fake, &world.manager).await;
    assert!(
        before
            .iter()
            .all(|u| u.presence == Presence::Missing && !u.is_up())
    );
    assert!(before[0].describe().contains("proxy: not installed"));

    let (world, fake) = installed(Platform::Launchd).await?;
    let after = collect(&fake, &world.manager).await;
    assert!(after.iter().all(|u| u.is_up()), "{after:?}");
    assert_eq!(after[0].activity, Activity::Running { pid: Some(77) });
    let text = after[0].describe();
    assert!(text.contains("installed as ai.temet.mike.proxy"), "{text}");
    assert!(text.contains("running (pid 77)"), "{text}");
    Ok(())
}

#[tokio::test]
async fn status_marks_a_foreign_file_and_does_not_ask_the_manager_about_it() -> anyhow::Result<()> {
    let world = World::new(Platform::Systemd)?;
    let path = world.file(ServiceKind::Proxy);
    std::fs::create_dir_all(path.parent().unwrap_or(world.path()))?;
    std::fs::write(&path, "[Unit]\nDescription=written by the user\n")?;
    let fake = Fake::new();
    let units = collect(&fake, &world.manager).await;
    assert_eq!(units[0].presence, Presence::Foreign);
    assert!(fake.seen().is_empty());
    assert!(units[0].describe().contains("mike did not write it"));
    Ok(())
}

#[test]
fn active_developer_units_are_read_from_the_systemctl_listing() {
    let listing = "mike-stack-proxy.service loaded active running mike\n  \nmike-stack-spa.service loaded active running spa\nsome-other.service loaded active running x\n";
    assert_eq!(
        active_dev_units(listing),
        ["mike-stack-proxy.service", "mike-stack-spa.service"]
    );
    assert!(active_dev_units("").is_empty());
}
