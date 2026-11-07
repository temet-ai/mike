use std::io::IsTerminal;

use clap::Parser;
use mike::cli::{BareDispatch, Command, bare_dispatch_target};
use mike::config::CliOverrides;

#[path = "../../build_info.rs"]
mod build_info;
mod fleet_invocation;

#[derive(Parser, Debug)]
#[command(
    name = "mike",
    version,
    about = "Reverse proxy for Claude API traffic",
    after_help = "Use --build-info alone for read-only JSON build identity."
)]
struct Cli {
    #[command(flatten)]
    overrides: CliOverrides,

    #[command(subcommand)]
    command: Option<Command>,
}

fn main() -> anyhow::Result<()> {
    let arguments: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    if mike::cli::is_version_request(&arguments) {
        println!("{}", mike::cli::version_line());
        return Ok(());
    }
    if build_info::requested(arguments.into_iter()) {
        build_info::print();
        return Ok(());
    }
    #[cfg(unix)]
    unsafe {
        libc::umask(0o077);
    }
    if fleet_invocation::background_hook(std::env::args_os().skip(1)) {
        std::process::exit(mike::launch::fleet::background_hook());
    }
    if fleet_invocation::receipt_hook(std::env::args_os().skip(1)) {
        std::process::exit(mike::launch::fleet::receipt_hook());
    }
    if let Some((program, args)) = fleet_invocation::timed_run(std::env::args_os().skip(1)) {
        std::process::exit(mike::launch::fleet::timed_run(program, args));
    }
    let cli = Cli::parse();
    mike::user_config::observe_selected(cli.overrides.config.as_deref())?;
    if cli.overrides.boot_config_snapshot.is_some() && !matches!(cli.command, Some(Command::Serve))
    {
        anyhow::bail!("private boot snapshots are accepted only by the owned serve launch");
    }
    run(cli)
}

#[tokio::main]
async fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Some(Command::Discover(args)) => mike::launch::fleet::discovery::run(args).await,
        Some(Command::Doctor(args)) => mike::doctor::run(args, cli.overrides).await,
        Some(Command::Setup(args)) => mike::setup::command::run(args, cli.overrides).await,
        Some(Command::Database(args)) => {
            let config = mike::config::load(cli.overrides)?;
            mike::setup::cli::run(args, &config).await
        }
        Some(Command::Mcp(args)) => mike::setup::mcp::run(args, cli.overrides).await,
        None => match bare_dispatch_target(std::io::stdout().is_terminal()) {
            BareDispatch::Tui => mike::tui::run(cli.overrides).await,
            BareDispatch::Serve => mike::serve::run(cli.overrides).await,
        },
        Some(Command::Serve) => mike::serve::run(cli.overrides).await,
        Some(Command::Claude(args)) => mike::claude::run(args.args, cli.overrides).await,
        Some(Command::ClaudeCode(args)) => {
            let code = mike::claude_code::run(args, cli.overrides).await;
            std::process::exit(code)
        }
        Some(Command::Features(args)) => {
            let code = mike::feature_presets::run(args, cli.overrides).await;
            std::process::exit(code)
        }
        Some(Command::Uninstall(args)) => {
            let code = mike::uninstall::run(args, cli.overrides).await;
            std::process::exit(code)
        }
        Some(Command::Spawn(args)) => {
            let code = mike::launch::cli::run(args, cli.overrides).await;
            std::process::exit(code)
        }
        Some(Command::Thread(args)) => mike::work_threads::run(args, cli.overrides).await,
        Some(Command::Fleet(args)) => {
            let code = mike::launch::fleet::run(args, cli.overrides).await;
            std::process::exit(code)
        }
        Some(Command::Config(cmd)) => mike::config::subcommands::run(cmd, cli.overrides).await,
        Some(Command::Db(args)) => {
            let code = mike::db_provision::run(args, cli.overrides).await;
            std::process::exit(code)
        }
        Some(Command::Service(args)) => {
            let code = mike::service::run(args, cli.overrides).await;
            std::process::exit(code)
        }
        Some(Command::Forward(args)) => mike::forward::run(args.args, cli.overrides).await,
        Some(Command::UserConfig(args)) => mike::user_config::run(args),
        Some(Command::ExportAnalysisCorpus(args)) => {
            mike::analysis::export_analysis_corpus(&args.out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::Parser;
    use mike::cli::Command;

    #[test]
    fn bare_invocation_has_no_subcommand() -> anyhow::Result<()> {
        let cli = Cli::try_parse_from(["mike"])?;
        assert!(cli.command.is_none());
        Ok(())
    }

    #[test]
    fn serve_subcommand_parses() -> anyhow::Result<()> {
        let cli = Cli::try_parse_from(["mike", "serve"])?;
        assert!(matches!(cli.command, Some(Command::Serve)));
        Ok(())
    }

    #[test]
    fn claude_forwards_trailing_flags() -> anyhow::Result<()> {
        let cli = Cli::try_parse_from(["mike", "claude", "-p", "hello", "--model", "opus"])?;
        let Some(Command::Claude(args)) = cli.command else {
            anyhow::bail!("expected claude subcommand");
        };
        assert_eq!(args.args, vec!["-p", "hello", "--model", "opus"]);
        Ok(())
    }

    #[test]
    fn claude_passes_args_after_double_dash() -> anyhow::Result<()> {
        let cli = Cli::try_parse_from(["mike", "claude", "--", "--version"])?;
        let Some(Command::Claude(args)) = cli.command else {
            anyhow::bail!("expected claude subcommand");
        };
        assert_eq!(args.args, vec!["--version"]);
        Ok(())
    }

    #[test]
    fn spawn_open_forwards_folder_flags_and_claude_args_in_order() -> anyhow::Result<()> {
        let cli = Cli::try_parse_from([
            "mike",
            "spawn",
            "open",
            "/tmp/proj",
            "--tab",
            "--model",
            "opus",
            "hi",
        ])?;
        let Some(Command::Spawn(spawn)) = cli.command else {
            anyhow::bail!("expected spawn subcommand");
        };
        let mike::launch::cli::SpawnCommand::Open(open) = spawn.command else {
            anyhow::bail!("expected spawn open");
        };
        assert_eq!(
            open.tokens,
            vec!["/tmp/proj", "--tab", "--model", "opus", "hi"]
        );
        Ok(())
    }

    #[test]
    fn spawn_open_keeps_leading_flags_and_double_dash() -> anyhow::Result<()> {
        let cli = Cli::try_parse_from([
            "mike",
            "spawn",
            "open",
            "--json",
            "/tmp/proj",
            "--",
            "--append-system-prompt",
            "--tab",
        ])?;
        let Some(Command::Spawn(spawn)) = cli.command else {
            anyhow::bail!("expected spawn subcommand");
        };
        let mike::launch::cli::SpawnCommand::Open(open) = spawn.command else {
            anyhow::bail!("expected spawn open");
        };
        assert_eq!(
            open.tokens,
            vec![
                "--json",
                "/tmp/proj",
                "--",
                "--append-system-prompt",
                "--tab"
            ]
        );
        Ok(())
    }

    #[test]
    fn spawn_kill_list_doctor_parse_their_flags() -> anyhow::Result<()> {
        use mike::launch::cli::SpawnCommand;
        let id = "123e4567-e89b-12d3-a456-426614174000";
        let cli = Cli::try_parse_from(["mike", "spawn", "kill", "--session", id, "--json"])?;
        let Some(Command::Spawn(spawn)) = cli.command else {
            anyhow::bail!("expected spawn");
        };
        let SpawnCommand::Kill(kill) = spawn.command else {
            anyhow::bail!("expected kill");
        };
        assert_eq!(kill.session.to_string(), id);
        assert!(kill.json);

        let cli = Cli::try_parse_from(["mike", "spawn", "list", "--run", "r1"])?;
        let Some(Command::Spawn(spawn)) = cli.command else {
            anyhow::bail!("expected spawn");
        };
        let SpawnCommand::List(list) = spawn.command else {
            anyhow::bail!("expected list");
        };
        assert_eq!(list.run.as_deref(), Some("r1"));
        assert!(!list.json);

        let cli = Cli::try_parse_from(["mike", "spawn", "doctor", "--json"])?;
        let Some(Command::Spawn(spawn)) = cli.command else {
            anyhow::bail!("expected spawn");
        };
        assert!(matches!(spawn.command, SpawnCommand::Doctor(d) if d.json));
        assert!(Cli::try_parse_from(["mike", "spawn", "open"]).is_err());
        assert!(Cli::try_parse_from(["mike", "spawn", "kill", "--session", "nope"]).is_err());
        Ok(())
    }

    #[test]
    fn fleet_subcommands_parse_their_flags() -> anyhow::Result<()> {
        use mike::launch::fleet::FleetCommand;
        let parse = |args: &[&str]| -> anyhow::Result<FleetCommand> {
            let cli = Cli::try_parse_from(args)?;
            let Some(Command::Fleet(fleet)) = cli.command else {
                anyhow::bail!("expected fleet subcommand");
            };
            Ok(fleet.command)
        };

        let FleetCommand::Status(status) =
            parse(&["mike", "fleet", "status", "x", "--watch", "5"])?
        else {
            anyhow::bail!("expected fleet status");
        };
        assert_eq!(status.fleet, "x");
        assert_eq!(status.watch, Some(5));
        assert!(!status.json);

        let FleetCommand::Status(status) = parse(&["mike", "fleet", "status", "x", "--json"])?
        else {
            anyhow::bail!("expected fleet status");
        };
        assert!(status.json);
        assert_eq!(status.watch, None);

        let FleetCommand::List(list) = parse(&["mike", "fleet", "list", "--project", "/p"])? else {
            anyhow::bail!("expected fleet list");
        };
        assert_eq!(list.project.as_deref(), Some("/p"));
        assert!(!list.json);

        let FleetCommand::Start(start) = parse(&["mike", "fleet", "start", "x"])? else {
            anyhow::bail!("expected fleet start");
        };
        assert_eq!(start.fleet, "x");

        assert!(Cli::try_parse_from(["mike", "fleet", "status", "x", "--watch", "0"]).is_err());
        assert!(Cli::try_parse_from(["mike", "fleet", "status"]).is_err());
        assert!(Cli::try_parse_from(["mike", "fleet", "start"]).is_err());
        Ok(())
    }

    #[test]
    fn config_subcommands_stay_top_level() -> anyhow::Result<()> {
        let cli = Cli::try_parse_from(["mike", "migrate-check"])?;
        assert!(matches!(cli.command, Some(Command::Config(_))));
        Ok(())
    }

    #[test]
    fn doctor_forward_and_config_subcommands_parse() -> anyhow::Result<()> {
        let cli = Cli::try_parse_from(["mike", "doctor", "--json"])?;
        assert!(matches!(cli.command, Some(Command::Doctor(d)) if d.json));
        let cli = Cli::try_parse_from(["mike", "forward", "--bind", "127.0.0.1:1"])?;
        let Some(Command::Forward(args)) = cli.command else {
            anyhow::bail!("expected forward");
        };
        assert_eq!(args.args, vec!["--bind", "127.0.0.1:1"]);
        let cli = Cli::try_parse_from(["mike", "forward", "ca-path"])?;
        assert!(matches!(cli.command, Some(Command::Forward(a)) if a.args == ["ca-path"]));
        let cli = Cli::try_parse_from(["mike", "config", "import-env", "/tmp/x.env"])?;
        assert!(matches!(cli.command, Some(Command::UserConfig(_))));
        let cli = Cli::try_parse_from(["mike", "config", "path", "--state"])?;
        assert!(matches!(cli.command, Some(Command::UserConfig(_))));
        assert!(Cli::try_parse_from(["mike", "config"]).is_err());
        Ok(())
    }

    fn parsed_db(args: &[&str]) -> anyhow::Result<String> {
        let cli = Cli::try_parse_from(args)?;
        let Some(Command::Db(db)) = cli.command else {
            anyhow::bail!("expected db subcommand");
        };
        Ok(format!("{:?}", db.command))
    }

    #[test]
    fn db_lifecycle_subcommands_parse_their_flags() -> anyhow::Result<()> {
        assert_eq!(
            parsed_db(&["mike", "db", "up"])?,
            "Up(UpArgs { port: None })"
        );
        assert_eq!(
            parsed_db(&["mike", "db", "up", "--port", "6701"])?,
            "Up(UpArgs { port: Some(6701) })"
        );
        assert_eq!(parsed_db(&["mike", "db", "down"])?, "Down");
        assert_eq!(parsed_db(&["mike", "db", "status"])?, "Status");
        assert_eq!(
            parsed_db(&["mike", "db", "native", "--yes"])?,
            "Native(NativeArgs { yes: true })"
        );
        assert!(Cli::try_parse_from(["mike", "db", "up", "--port", "99999"]).is_err());
        assert!(Cli::try_parse_from(["mike", "db"]).is_err());
        Ok(())
    }

    #[test]
    fn db_check_takes_an_optional_url_and_save_needs_one() -> anyhow::Result<()> {
        assert_eq!(
            parsed_db(&["mike", "db", "check"])?,
            "Check(CheckArgs { url: None, save: false })"
        );
        assert_eq!(
            parsed_db(&["mike", "db", "check", "postgres://u@h/d", "--save"])?,
            "Check(CheckArgs { url: Some(\"postgres://u@h/d\"), save: true })"
        );
        assert!(Cli::try_parse_from(["mike", "db", "check", "--save"]).is_err());
        Ok(())
    }

    #[test]
    fn export_analysis_corpus_subcommand_parses() -> anyhow::Result<()> {
        let cli = Cli::try_parse_from(["mike", "export-analysis-corpus", "--out", "/tmp/x.json"])?;
        let Some(Command::ExportAnalysisCorpus(args)) = cli.command else {
            anyhow::bail!("expected export-analysis-corpus subcommand");
        };
        assert_eq!(args.out, std::path::PathBuf::from("/tmp/x.json"));
        Ok(())
    }

    mod routers {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use mike::app::{inspect_router, proxy_router};
        use mike::config::{AppState, ProxyConfig};
        use serde_json::Value;
        use tower::ServiceExt;

        fn test_config() -> ProxyConfig {
            ProxyConfig::for_tests_in_memory()
        }

        async fn test_state() -> anyhow::Result<AppState> {
            AppState::new(test_config()).await
        }

        #[tokio::test]
        async fn test_health() -> anyhow::Result<()> {
            let app = inspect_router(test_state().await?);
            let response = app
                .oneshot(
                    Request::builder()
                        .header("host", "127.0.0.1:9989")
                        .uri("/api/health")
                        .body(Body::empty())?,
                )
                .await?;

            assert_eq!(response.status(), StatusCode::OK);

            let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
            let value: Value = serde_json::from_slice(&body)?;
            assert!(
                value.get("status").and_then(Value::as_str).is_some(),
                "health must carry a status; got {value}"
            );
            Ok(())
        }

        #[tokio::test]
        async fn test_proxy_router_rejects_api_paths() -> anyhow::Result<()> {
            let app = proxy_router(test_state().await?);
            let response = app
                .oneshot(
                    Request::builder()
                        .header("host", "127.0.0.1:9989")
                        .uri("/api/health")
                        .body(Body::empty())?,
                )
                .await?;
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
            Ok(())
        }

        #[tokio::test]
        async fn test_inspect_router_rejects_v1_paths() -> anyhow::Result<()> {
            let app = inspect_router(test_state().await?);
            let response = app
                .oneshot(
                    Request::builder()
                        .header("host", "127.0.0.1:9989")
                        .uri("/v1/messages")
                        .method("POST")
                        .body(Body::empty())?,
                )
                .await?;
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
            Ok(())
        }
    }
}
