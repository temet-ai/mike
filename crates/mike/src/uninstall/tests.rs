use std::io;
use std::io::Cursor;
use std::sync::Mutex;

use serde_json::Value;

use super::purge::purge_with;
use super::remove_connection;
use crate::claude_code::{
    Locations, Route, Shell, ShellRequest, add_function, add_mods, choose_route, choose_trust,
};
use crate::db_provision::{Call, Captured, Paths, Runner, Timing};

struct Fake {
    calls: Mutex<Vec<String>>,
    fail_volume: bool,
}

impl Fake {
    fn new(fail_volume: bool) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            fail_volume,
        }
    }

    fn seen(&self) -> Vec<String> {
        self.calls
            .lock()
            .map(|calls| calls.clone())
            .unwrap_or_default()
    }
}

impl Runner for Fake {
    async fn run(&self, call: &Call) -> io::Result<Captured> {
        if let Ok(mut calls) = self.calls.lock() {
            calls.push(call.shown());
        }
        let failing = self.fail_volume && call.shown().contains("volume rm");
        Ok(Captured {
            success: !failing,
            stdout: String::new(),
            stderr: if failing {
                "volume in use".to_string()
            } else {
                String::new()
            },
        })
    }
}

fn paths(dir: &tempfile::TempDir) -> Paths {
    Paths {
        config_file: dir.path().join("config.toml"),
        state_dir: dir.path().join("state"),
    }
}

async fn purge(
    fake: &Fake,
    dir: &tempfile::TempDir,
    answers: &str,
) -> anyhow::Result<(i32, String)> {
    let mut input = Cursor::new(answers.as_bytes().to_vec());
    let mut output = Vec::new();
    let code = purge_with(fake, &paths(dir), Timing::REAL, &mut input, &mut output).await?;
    Ok((code, String::from_utf8(output)?))
}

#[tokio::test]
async fn purge_deletes_the_volume_and_the_config_file_after_two_yes_answers() -> anyhow::Result<()>
{
    let dir = tempfile::tempdir()?;
    std::fs::write(dir.path().join("config.toml"), "database_url = \"x\"\n")?;
    let fake = Fake::new(false);
    let (code, said) = purge(&fake, &dir, "y\nyes\n").await?;
    assert_eq!(code, 0, "{said}");
    assert_eq!(fake.seen(), vec!["docker volume rm mike_pg_data"]);
    assert!(!dir.path().join("config.toml").exists());
    assert!(
        said.contains("Deleted the Docker volume mike_pg_data"),
        "{said}"
    );
    assert!(said.contains("never set up a database here"), "{said}");
    Ok(())
}

#[tokio::test]
async fn purge_keeps_everything_on_no_and_on_an_empty_answer() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    std::fs::write(dir.path().join("config.toml"), "x = 1\n")?;
    for answers in ["n\nn\n", "\n\n", ""] {
        let fake = Fake::new(false);
        let (code, said) = purge(&fake, &dir, answers).await?;
        assert_eq!(code, 0, "{said}");
        assert!(fake.seen().is_empty(), "{answers:?}: {:?}", fake.seen());
        assert!(dir.path().join("config.toml").exists());
        assert!(said.contains("Kept the Docker volume"), "{said}");
    }
    Ok(())
}

#[tokio::test]
async fn purge_reports_a_volume_that_cannot_be_removed_and_exits_with_one() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let fake = Fake::new(true);
    let (code, said) = purge(&fake, &dir, "y\nn\n").await?;
    assert_eq!(code, 1);
    assert!(said.contains("Could not delete the volume"), "{said}");
    assert!(said.contains("volume in use"), "{said}");
    Ok(())
}

#[tokio::test]
async fn purge_stops_a_database_that_mike_db_up_set_up() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let compose = dir.path().join("state").join("db");
    std::fs::create_dir_all(&compose)?;
    std::fs::write(compose.join("docker-compose.yml"), "services: {}\n")?;
    let fake = Fake::new(false);
    let (_, said) = purge(&fake, &dir, "n\nn\n").await?;
    let seen = fake.seen().join("\n");
    assert!(seen.contains("docker info"), "{seen}");
    assert!(
        said.contains("Stopped the database container.") || said.contains("Could not stop"),
        "{said}"
    );
    Ok(())
}

#[tokio::test]
async fn removing_the_connection_takes_out_only_what_mike_wrote() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let root = std::fs::canonicalize(dir.path())?;
    let home = root.join("home");
    let bin = root.join("bin");
    std::fs::create_dir_all(&home)?;
    std::fs::create_dir_all(&bin)?;
    let mcp = bin.join("mike-mcp");
    std::fs::write(&mcp, "#!/bin/sh\n")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&mcp, std::fs::Permissions::from_mode(0o755))?;
    }
    let locations = Locations::for_home(&home, Some(bin.clone().into_os_string()));
    std::fs::write(
        locations.claude_json(),
        r#"{"theme":"dark","mcpServers":{"other":{"command":"x"}}}"#,
    )?;
    crate::claude_code::connect_mcp(&locations, "http://127.0.0.1:9989", false).await?;
    choose_route(&locations, Route::Sidecar)?;
    choose_trust(&locations, true)?;
    std::fs::write(home.join(".bashrc"), "alias ll='ls -l'\n")?;
    let request = ShellRequest {
        name: "mcc".to_string(),
        shells: vec![Shell::Bash],
        route: Route::Sidecar,
        proxy_addr: "127.0.0.1:9988".to_string(),
    };
    add_function(&locations, &request, None)?;
    let mods = root.join("share");
    std::fs::create_dir_all(mods.join("mike").join(".claude-plugin"))?;
    std::fs::write(
        mods.join("mike").join(".claude-plugin").join("plugin.json"),
        "{}",
    )?;
    std::fs::create_dir_all(locations.claude_settings().parent().unwrap_or(&home))?;
    std::fs::write(
        locations.claude_settings(),
        r#"{"model":"opus","env":{"CLAUDE_CODE_PLUGIN_DIRS":"/foreign"}}"#,
    )?;
    add_mods(&locations, &mods, false)?;

    let findings = remove_connection(&locations)?;
    assert!(crate::finding::all_passed(&findings), "{findings:?}");

    let claude: Value = serde_json::from_str(&std::fs::read_to_string(locations.claude_json())?)?;
    assert_eq!(claude["theme"], "dark");
    assert!(claude["mcpServers"].get("mike").is_none());
    assert_eq!(claude["mcpServers"]["other"]["command"], "x");
    let rc = std::fs::read_to_string(home.join(".bashrc"))?;
    assert_eq!(rc, "alias ll='ls -l'\n");
    let settings: Value =
        serde_json::from_str(&std::fs::read_to_string(locations.claude_settings())?)?;
    assert_eq!(settings["model"], "opus");
    assert_eq!(settings["env"]["CLAUDE_CODE_PLUGIN_DIRS"], "/foreign");
    let config = std::fs::read_to_string(&locations.config_file)?;
    assert!(!config.contains("claude_route"), "{config}");
    assert!(!config.contains("claude_function"), "{config}");
    assert!(!config.contains("claude_mods"), "{config}");
    assert!(
        config.contains("launch_auto_trust = true"),
        "kept: {config}"
    );
    Ok(())
}
