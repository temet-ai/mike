use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::{inputs, manager};
use crate::db_provision::Captured;
use crate::service::binary::{homebrew_link, stable_path};
use crate::service::environment::{service_environment, service_path};
use crate::service::platform::Platform;
use crate::service::spec::ServiceKind;
use crate::service::state::{Activity, from_launchctl_print, from_systemctl_show};

fn captured(success: bool, stdout: &str) -> Captured {
    Captured {
        success,
        stdout: stdout.to_string(),
        stderr: String::new(),
    }
}

#[test]
fn a_cellar_binary_maps_to_the_symlink_homebrew_repoints_on_upgrade() {
    for (cellar, link) in [
        (
            "/opt/homebrew/Cellar/mike/1.2.3/bin/mike",
            "/opt/homebrew/bin/mike",
        ),
        (
            "/home/linuxbrew/.linuxbrew/Cellar/mike/0.9.0/bin/mike",
            "/home/linuxbrew/.linuxbrew/bin/mike",
        ),
        (
            "/usr/local/Cellar/mike/2.0.0_1/bin/mike",
            "/usr/local/bin/mike",
        ),
    ] {
        assert_eq!(
            homebrew_link(Path::new(cellar)),
            Some(PathBuf::from(link)),
            "{cellar}"
        );
    }
}

#[test]
fn paths_outside_a_cellar_binary_folder_are_not_mapped() {
    for path in [
        "/usr/local/bin/mike",
        "/opt/homebrew/Cellar/mike/1.2.3/libexec/mike",
        "/opt/homebrew/Cellar/mike/bin/mike",
        "/opt/homebrew/Cellar/mike/1.2.3/bin/sub/mike",
        "/home/ada/.local/share/mise/installs/mike/1.2.3/bin/mike",
    ] {
        assert_eq!(homebrew_link(Path::new(path)), None, "{path}");
    }
}

fn fake_cellar(root: &Path, version: &str) -> anyhow::Result<PathBuf> {
    let bin = root.join("Cellar/mike").join(version).join("bin");
    std::fs::create_dir_all(&bin)?;
    let binary = bin.join("mike");
    std::fs::write(&binary, "binary")?;
    Ok(binary)
}

#[test]
fn stable_path_uses_the_prefix_symlink_when_it_points_at_the_running_binary() -> anyhow::Result<()>
{
    let root = tempfile::tempdir()?;
    let canonical_root = std::fs::canonicalize(root.path())?;
    let binary = fake_cellar(&canonical_root, "1.0.0")?;
    std::fs::create_dir_all(canonical_root.join("bin"))?;
    std::os::unix::fs::symlink(&binary, canonical_root.join("bin/mike"))?;
    assert_eq!(stable_path(&binary)?, canonical_root.join("bin/mike"));
    Ok(())
}

#[test]
fn stable_path_keeps_the_versioned_path_when_the_symlink_points_elsewhere() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let canonical_root = std::fs::canonicalize(root.path())?;
    let running = fake_cellar(&canonical_root, "1.0.0")?;
    let other = fake_cellar(&canonical_root, "2.0.0")?;
    std::fs::create_dir_all(canonical_root.join("bin"))?;
    std::os::unix::fs::symlink(&other, canonical_root.join("bin/mike"))?;
    assert_eq!(stable_path(&running)?, running);
    Ok(())
}

#[test]
fn stable_path_of_a_missing_binary_is_an_error() {
    assert!(stable_path(Path::new("/definitely/not/here/mike")).is_err());
}

#[test]
fn stable_path_of_a_plain_binary_is_its_resolved_path() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let canonical_root = std::fs::canonicalize(root.path())?;
    let binary = canonical_root.join("mike");
    std::fs::write(&binary, "binary")?;
    assert_eq!(stable_path(&binary)?, binary);
    Ok(())
}

#[test]
fn the_service_path_holds_homebrew_user_and_mise_directories_in_a_fixed_order() {
    let home = Path::new("/home/ada");
    let path = service_path(&inputs(home));
    let dirs: Vec<&str> = path.split(':').collect();
    assert_eq!(dirs[0], "/home/ada/.local/bin");
    assert_eq!(dirs[1], "/home/ada/.local/share/mise/shims");
    for expected in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/sbin"] {
        assert!(dirs.contains(&expected), "{expected} missing from {path}");
    }
    assert_eq!(dirs.last().copied(), Some("/sbin"));
}

#[test]
fn a_mise_data_directory_moves_the_shims_folder() {
    let mut given = inputs(Path::new("/home/ada"));
    given.mise_data_dir = Some("/data/mise".to_string());
    assert!(service_path(&given).starts_with("/home/ada/.local/bin:/data/mise/shims:"));
    given.mise_data_dir = Some("relative/mise".to_string());
    assert!(service_path(&given).contains("/home/ada/.local/share/mise/shims"));
}

#[test]
fn the_folders_holding_claude_and_codex_on_the_install_path_are_added_once() -> anyhow::Result<()> {
    let tools = tempfile::tempdir()?;
    for name in ["claude", "codex"] {
        let file = tools.path().join(name);
        std::fs::write(&file, "#!/bin/sh\n")?;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755))?;
    }
    let mut given = inputs(Path::new("/home/ada"));
    given.path_var = Some(std::env::join_paths([tools.path()])?);
    let path = service_path(&given);
    let wanted = tools.path().display().to_string();
    assert_eq!(
        path.split(':').filter(|d| *d == wanted).count(),
        1,
        "{path}"
    );
    Ok(())
}

#[test]
fn only_absolute_xdg_directories_are_passed_to_the_service() {
    let mut given = inputs(Path::new("/home/ada"));
    given.xdg_config_home = Some("/cfg".to_string());
    given.xdg_state_home = Some("relative".to_string());
    let names: Vec<String> = service_environment(&given)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(names, ["PATH", "XDG_CONFIG_HOME"]);
}

#[test]
fn launchd_files_live_in_launch_agents_and_systemd_files_follow_xdg_config_home() {
    let home = Path::new("/home/ada");
    let mac = manager(Platform::Launchd, home);
    assert_eq!(
        mac.file_path(ServiceKind::Proxy),
        PathBuf::from("/home/ada/Library/LaunchAgents/ai.temet.mike.proxy.plist")
    );
    assert_eq!(
        mac.file_path(ServiceKind::Sidecar),
        PathBuf::from("/home/ada/Library/LaunchAgents/ai.temet.mike.forward.plist")
    );
    let linux = manager(Platform::Systemd, home);
    assert_eq!(
        linux.file_path(ServiceKind::Proxy),
        PathBuf::from("/home/ada/.config/systemd/user/mike.service")
    );
    let custom = crate::service::manager::Manager::new(Platform::Systemd, home, Some("/xdg"), 501);
    assert_eq!(
        custom.file_path(ServiceKind::Sidecar),
        PathBuf::from("/xdg/systemd/user/mike-forward.service")
    );
}

#[test]
fn unit_names_do_not_collide_with_the_developer_stack() {
    for kind in ServiceKind::ALL {
        assert!(!kind.systemd_unit().starts_with("mike-stack-"));
    }
}

#[test]
fn the_install_calls_follow_what_is_loaded_and_whether_the_file_changed() {
    let home = Path::new("/home/ada");
    let mac = manager(Platform::Launchd, home);
    let shown = |calls: Vec<crate::db_provision::Call>| -> Vec<String> {
        calls.iter().map(crate::db_provision::Call::shown).collect()
    };
    let plist = "/home/ada/Library/LaunchAgents/ai.temet.mike.proxy.plist";
    let running = Activity::Running { pid: Some(5) };
    assert_eq!(
        shown(mac.install_calls(ServiceKind::Proxy, &Activity::NotLoaded, true)),
        [format!("launchctl bootstrap gui/501 {plist}")]
    );
    assert_eq!(
        shown(mac.install_calls(ServiceKind::Proxy, &running, true)),
        [
            "launchctl bootout gui/501/ai.temet.mike.proxy".to_string(),
            format!("launchctl bootstrap gui/501 {plist}")
        ]
    );
    assert!(
        mac.install_calls(ServiceKind::Proxy, &running, false)
            .is_empty()
    );
    assert_eq!(
        shown(mac.install_calls(
            ServiceKind::Proxy,
            &Activity::Stopped("waiting".to_string()),
            false
        )),
        ["launchctl kickstart gui/501/ai.temet.mike.proxy"]
    );
    let linux = manager(Platform::Systemd, home);
    assert_eq!(
        shown(linux.install_calls(ServiceKind::Proxy, &Activity::NotLoaded, true)),
        [
            "systemctl --user daemon-reload",
            "systemctl --user enable --now mike.service"
        ]
    );
    assert_eq!(
        shown(linux.install_calls(ServiceKind::Proxy, &running, true)),
        [
            "systemctl --user daemon-reload",
            "systemctl --user enable --now mike.service",
            "systemctl --user restart mike.service"
        ]
    );
}

const LAUNCHCTL_RUNNING: &str = "gui/501/ai.temet.mike.proxy = {\n\tactive count = 1\n\tpath = /x.plist\n\tstate = running\n\n\tprogram = /opt/homebrew/bin/mike\n\tpid = 4242\n}\n";

#[test]
fn launchctl_print_output_becomes_running_stopped_or_not_loaded() {
    assert_eq!(
        from_launchctl_print(&captured(true, LAUNCHCTL_RUNNING)),
        Activity::Running { pid: Some(4242) }
    );
    assert_eq!(
        from_launchctl_print(&captured(true, "\tstate = spawn scheduled\n")),
        Activity::Stopped("spawn scheduled".to_string())
    );
    assert_eq!(
        from_launchctl_print(&captured(false, "")),
        Activity::NotLoaded
    );
}

#[test]
fn systemctl_show_output_becomes_running_stopped_or_not_loaded() {
    let running = "LoadState=loaded\nActiveState=active\nSubState=running\nMainPID=991\n";
    assert_eq!(
        from_systemctl_show(&captured(true, running)),
        Activity::Running { pid: Some(991) }
    );
    let failed = "LoadState=loaded\nActiveState=failed\nSubState=failed\nMainPID=0\n";
    assert_eq!(
        from_systemctl_show(&captured(true, failed)),
        Activity::Stopped("failed, failed".to_string())
    );
    let missing = "LoadState=not-found\nActiveState=inactive\nSubState=dead\nMainPID=0\n";
    assert_eq!(
        from_systemctl_show(&captured(true, missing)),
        Activity::NotLoaded
    );
    assert!(matches!(
        from_systemctl_show(&captured(false, "")),
        Activity::Unknown(_)
    ));
}
