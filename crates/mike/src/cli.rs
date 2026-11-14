use clap::{Args, Subcommand};

use crate::config::subcommands::ConfigCommand;

pub mod structural;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BareDispatch {
    Tui,
    Serve,
}

pub fn bare_dispatch_target(stdout_is_tty: bool) -> BareDispatch {
    if stdout_is_tty {
        BareDispatch::Tui
    } else {
        BareDispatch::Serve
    }
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    Serve,

    Doctor(crate::doctor::DoctorArgs),

    Setup(crate::setup::command::SetupArgs),

    Database(crate::setup::cli::DatabaseArgs),

    Mcp(crate::setup::mcp::McpArgs),

    Claude(ClaudeArgs),

    ClaudeCode(crate::claude_code::ClaudeCodeArgs),

    Features(crate::feature_presets::FeaturesArgs),

    Uninstall(crate::uninstall::UninstallArgs),

    Spawn(crate::launch::cli::SpawnArgs),

    Thread(crate::work_threads::ThreadArgs),

    Fleet(crate::launch::fleet::FleetArgs),

    Discover(crate::launch::fleet::discovery::DiscoveryArgs),

    #[command(flatten)]
    Config(ConfigCommand),

    Db(crate::db_provision::DbArgs),

    Service(crate::service::ServiceArgs),

    Forward(ForwardArgs),

    #[command(name = "config")]
    UserConfig(crate::user_config::UserConfigArgs),

    ExportAnalysisCorpus(ExportAnalysisCorpusArgs),

    Structural(structural::StructuralArgs),
}

#[derive(Debug, Clone, Args)]
pub struct ForwardArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, num_args = 0..)]
    pub args: Vec<String>,
}

pub fn version_line() -> String {
    let build = crate::change_log::current_build();
    let dirty = if build.dirty { "-dirty" } else { "" };
    format!(
        "mike {} ({}{dirty}, built {})",
        env!("CARGO_PKG_VERSION"),
        build.sha_short,
        build.built_at
    )
}

pub fn is_version_request(args: &[std::ffi::OsString]) -> bool {
    matches!(args, [only] if only == "--version" || only == "-V")
}

#[derive(Debug, Clone, Args)]
pub struct ExportAnalysisCorpusArgs {
    #[arg(long)]
    pub out: std::path::PathBuf,
}

#[derive(Debug, Clone, Args)]
pub struct ClaudeArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, num_args = 0..)]
    pub args: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    use clap::Parser;

    #[derive(Parser, Debug)]
    struct TestCli {
        #[command(subcommand)]
        command: Option<Command>,
    }

    #[test]
    fn claude_code_subcommands_parse_with_their_options() -> anyhow::Result<()> {
        use crate::claude_code::{ClaudeCodeCommand, Route, Shell};
        let parse = |args: &[&str]| -> anyhow::Result<ClaudeCodeCommand> {
            let mut all = vec!["mike", "claude-code"];
            all.extend_from_slice(args);
            match TestCli::try_parse_from(all)?.command {
                Some(Command::ClaudeCode(parsed)) => Ok(parsed.command),
                other => anyhow::bail!("expected claude-code, got {other:?}"),
            }
        };
        assert!(matches!(parse(&["status"])?, ClaudeCodeCommand::Status));
        let ClaudeCodeCommand::Mcp(mcp) = parse(&["mcp", "--no-check"])? else {
            anyhow::bail!("expected mcp");
        };
        assert!(mcp.no_check && !mcp.remove);
        let ClaudeCodeCommand::Route(route) = parse(&["route", "plain"])? else {
            anyhow::bail!("expected route");
        };
        assert_eq!(route.route, Route::Plain);
        let ClaudeCodeCommand::Shell(shell) =
            parse(&["shell", "--name", "mcc", "--shell", "zsh,fish"])?
        else {
            anyhow::bail!("expected shell");
        };
        assert_eq!(shell.name.as_deref(), Some("mcc"));
        assert_eq!(shell.shells, vec![Shell::Zsh, Shell::Fish]);
        let ClaudeCodeCommand::Mods(mods) = parse(&["mods", "--with-wiretap"])? else {
            anyhow::bail!("expected mods");
        };
        assert!(mods.with_wiretap);
        let ClaudeCodeCommand::Trust(trust) = parse(&["trust", "off"])? else {
            anyhow::bail!("expected trust");
        };
        assert_eq!(trust.setting, "off");
        assert!(parse(&["trust", "maybe"]).is_err());
        assert!(parse(&["route", "sideways"]).is_err());
        Ok(())
    }

    #[test]
    fn setup_takes_an_optional_step_and_uninstall_an_optional_purge() -> anyhow::Result<()> {
        let Some(Command::Setup(all)) = TestCli::try_parse_from(["mike", "setup"])?.command else {
            anyhow::bail!("expected setup");
        };
        assert_eq!(all.step, None);
        let Some(Command::Setup(one)) =
            TestCli::try_parse_from(["mike", "setup", "claude"])?.command
        else {
            anyhow::bail!("expected setup");
        };
        assert_eq!(one.step.as_deref(), Some("claude"));
        let Some(Command::Uninstall(plain)) =
            TestCli::try_parse_from(["mike", "uninstall"])?.command
        else {
            anyhow::bail!("expected uninstall");
        };
        assert!(!plain.purge);
        let Some(Command::Uninstall(purge)) =
            TestCli::try_parse_from(["mike", "uninstall", "--purge"])?.command
        else {
            anyhow::bail!("expected uninstall");
        };
        assert!(purge.purge);
        Ok(())
    }

    #[test]
    fn version_request_is_the_lone_version_flag() {
        let os = |args: &[&str]| args.iter().map(Into::into).collect::<Vec<_>>();
        assert!(is_version_request(&os(&["--version"])));
        assert!(is_version_request(&os(&["-V"])));
        assert!(!is_version_request(&os(&[])));
        assert!(!is_version_request(&os(&["--version", "serve"])));
        assert!(!is_version_request(&os(&["serve", "--version"])));
    }

    #[test]
    fn version_line_names_the_version_and_the_commit() {
        let line = version_line();
        assert!(line.starts_with("mike "), "{line}");
        assert!(line.contains(env!("CARGO_PKG_VERSION")), "{line}");
        assert!(
            line.contains(crate::change_log::current_build().sha_short),
            "{line}"
        );
    }

    #[test]
    fn tty_stdout_opens_the_tui() {
        assert_eq!(bare_dispatch_target(true), BareDispatch::Tui);
    }

    #[test]
    fn non_tty_stdout_serves_headless() {
        assert_eq!(bare_dispatch_target(false), BareDispatch::Serve);
    }
}

#[cfg(test)]
mod cross_crate_tests;
