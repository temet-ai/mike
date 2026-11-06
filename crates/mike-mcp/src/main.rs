use anyhow::{Context, bail};
use mike_mcp::{Profile, run_stdio_server};
use tracing_subscriber::EnvFilter;

#[path = "../../build_info.rs"]
mod build_info;

fn resolve_server_profile(cli_flag: Option<&str>) -> anyhow::Result<Profile> {
    if let Some(flag) = cli_flag {
        return Profile::parse(flag)
            .with_context(|| format!("unknown profile {flag:?} (use {})", Profile::NAMES));
    }
    match std::env::var("MIKE_MCP_PROFILE") {
        Ok(v) if !v.trim().is_empty() => Profile::parse(&v).with_context(|| {
            format!(
                "MIKE_MCP_PROFILE={v:?} is not a valid profile ({})",
                Profile::NAMES
            )
        }),
        _ => Ok(Profile::default()),
    }
}

fn profile_flag(arg: &str) -> Option<Profile> {
    arg.strip_prefix("--").and_then(Profile::parse)
}

fn list_tools(args: impl Iterator<Item = String>) -> anyhow::Result<()> {
    let mut as_json = false;
    let mut profile = Profile::Full;
    for arg in args {
        if arg == "--json" {
            as_json = true;
        } else if let Some(chosen) = profile_flag(&arg) {
            profile = chosen;
        } else {
            bail!(
                "unknown argument for list-tools: {arg} \
                 (supported: --json, --{})",
                Profile::NAMES.replace('|', ", --")
            );
        }
    }
    if as_json {
        let json = mike_mcp::cli::render_tools_json(profile)
            .context("failed to serialize the tool list as JSON")?;
        println!("{json}");
    } else {
        print!("{}", mike_mcp::cli::render_tools_text(profile));
    }
    Ok(())
}

fn no_extra(mut args: impl Iterator<Item = String>) -> anyhow::Result<()> {
    if let Some(extra) = args.next() {
        bail!("unexpected extra argument: {extra}");
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    if build_info::requested(std::env::args_os().skip(1)) {
        build_info::print();
        return Ok(());
    }
    run()
}

#[tokio::main]
async fn run() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut cli_profile: Option<String> = None;
    match args.next().as_deref() {
        Some("--version" | "-V") => {
            no_extra(args)?;
            println!("mike-mcp {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("--help" | "-h") => {
            no_extra(args)?;
            println!(
                "mike-mcp {}\n\nUsage: mike-mcp [serve] [--<profile>]\n       mike-mcp list-tools [--json] [--<profile>]\n\nProfiles: {}\n\nOptions:\n  -h, --help     Print help\n  -V, --version  Print version\n  --build-info   Print read-only JSON build identity (alone)\n\nNo arguments starts the stdio MCP server. Diagnostics use stderr.",
                env!("CARGO_PKG_VERSION"),
                Profile::NAMES
            );
            return Ok(());
        }
        Some("list-tools") => return list_tools(args),
        Some("serve") => {
            if let Some(flag) = args.next() {
                if profile_flag(&flag).is_none() {
                    bail!(
                        "unknown argument for serve: {flag} (use --{})",
                        Profile::NAMES.replace('|', ", --")
                    );
                }
                cli_profile = Some(flag.trim_start_matches("--").to_string());
            }
            no_extra(args)?;
        }
        Some(flag) if profile_flag(flag).is_some() => {
            cli_profile = Some(flag.trim_start_matches("--").to_string());
            no_extra(args)?;
        }
        Some(other) => bail!(
            "unknown subcommand: {other}; run with no arguments (or `serve`) for stdio MCP mode, \
             optionally with `--<profile>` ({}), or use `list-tools [--json] [--<profile>]`",
            Profile::NAMES
        ),
        None => {}
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .with_target(true)
        .init();

    let base_url =
        std::env::var("MIKE_INSPECT_URL").unwrap_or_else(|_| "http://127.0.0.1:9989".to_string());
    let profile = resolve_server_profile(cli_profile.as_deref())?;
    let session_uuid = caller_session(
        std::env::var("CLAUDE_CODE_SESSION_ID").ok(),
        std::env::var("MIKE_SESSION_ID").ok(),
    );
    tracing::info!(
        %base_url,
        version = env!("CARGO_PKG_VERSION"),
        profile = ?profile,
        channel_session = session_uuid.as_deref().unwrap_or("<unset>"),
        "mike-mcp starting: inspect base URL + tool profile resolved"
    );

    run_stdio_server(base_url, profile, session_uuid)
        .await
        .context("mike-mcp server loop exited with an error")?;

    tracing::info!("mike-mcp exiting normally");
    Ok(())
}

fn caller_session(claude: Option<String>, mike: Option<String>) -> Option<String> {
    claude
        .filter(|value| !value.trim().is_empty())
        .or_else(|| mike.filter(|value| !value.trim().is_empty()))
        .map(|value| value.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::caller_session;

    #[test]
    fn caller_session_prefers_current_claude_over_inherited_mike() {
        assert_eq!(
            caller_session(Some("current".into()), Some("parent".into())),
            Some("current".into())
        );
        assert_eq!(
            caller_session(None, Some("launch".into())),
            Some("launch".into())
        );
        assert_eq!(
            caller_session(Some(" ".into()), Some("launch".into())),
            Some("launch".into())
        );
        assert_eq!(caller_session(None, None), None);
    }
}
