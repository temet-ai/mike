use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Context;
use uuid::Uuid;

use crate::claude_code::{Route, stored_route};
use crate::config::{CliOverrides, ProxyConfig};
use crate::launch::{ProxyEnv, TrustGate, TrustOutcome};

#[derive(Debug, Clone)]
pub struct ClaudeWiring<'a> {
    pub proxy_addr: &'a str,
    pub sidecar: Option<&'a ProxyEnv>,
    pub operator_args: &'a [String],
}

#[derive(Debug, PartialEq, Eq)]
pub struct ClaudeInvocation {
    pub env: BTreeMap<String, String>,
    pub args: Vec<String>,
    pub session: Option<Uuid>,
}

pub async fn run(args: Vec<String>, overrides: CliOverrides) -> anyhow::Result<()> {
    let config = crate::config::load(overrides).context("failed to load configuration")?;

    let outcome = crate::stack::ensure_up(&config).await?;

    print!("{}", crate::stack::render_stack_summary(&config, &outcome));
    println!("  Launching claude…");

    let cwd = std::env::current_dir().context("reading the working directory")?;
    let sidecar = sidecar_env(&config, &cwd).await;
    trust_cwd(&config, &cwd).await;
    let wiring = ClaudeWiring {
        proxy_addr: &config.proxy_listen_addr,
        sidecar: sidecar.as_ref(),
        operator_args: &config.launch.claude_args,
    };
    let invocation = build_claude_invocation(&wiring, &args, Uuid::new_v4());
    if let Some(id) = invocation.session {
        eprintln!("session: {id}");
    }
    use std::io::Write;
    std::io::stdout().flush().ok();
    exec_claude(&invocation)
}

fn wants_sidecar(route: Option<Route>, use_sidecar: bool) -> bool {
    match route {
        Some(Route::Sidecar) => true,
        Some(Route::Plain) => false,
        None => use_sidecar,
    }
}

async fn sidecar_env(config: &ProxyConfig, cwd: &Path) -> Option<ProxyEnv> {
    let route = stored_route(&config.config_path).ok().flatten();
    if !wants_sidecar(route, config.launch.use_sidecar) {
        return None;
    }
    let url = &config.launch.proxy_forward_url;
    let resolved = crate::launch::cli::resolve_proxy_env(&config.launch, Some(cwd));
    let why = match (&resolved, crate::launch::cli::sidecar_reachable(url).await) {
        (Some(_), Ok(_)) => return resolved,
        (None, _) => "no CA certificate found; set MIKE_LAUNCH_CA_CERT".to_string(),
        (Some(_), Err(e)) => e,
    };
    eprintln!(
        "mike claude: sidecar not reachable on {url}; captured through the proxy but the \
         session will have no Remote Control channel ({why})"
    );
    None
}

async fn trust_cwd(config: &ProxyConfig, cwd: &Path) {
    let gate = match TrustGate::from_home() {
        Ok(gate) => gate,
        Err(e) => {
            eprintln!("mike claude: trust gate unavailable: {e}");
            return;
        }
    };
    match gate.ensure(cwd, config.launch.auto_trust).await {
        Ok(TrustOutcome::Trusted { .. }) => {}
        Ok(TrustOutcome::Written { target }) => {
            eprintln!("mike claude: recorded {} as trusted", target.display());
        }
        Ok(TrustOutcome::Untrusted { target, remedy }) => {
            eprintln!(
                "mike claude: {} is not trusted; the trust dialog will appear ({remedy})",
                target.display()
            );
        }
        Ok(TrustOutcome::Unknown { detail, .. }) => {
            eprintln!("mike claude: trust state unknown: {detail}");
        }
        Err(e) => eprintln!("mike claude: trust gate failed: {e}"),
    }
}

pub fn build_claude_invocation(
    wiring: &ClaudeWiring<'_>,
    user_args: &[String],
    new_session: Uuid,
) -> ClaudeInvocation {
    let session = (!carries_session(user_args)).then_some(new_session);
    let mut args = Vec::with_capacity(user_args.len() + wiring.operator_args.len() + 3);
    if let Some(id) = session {
        args.push("--session-id".to_string());
        args.push(id.to_string());
    }
    args.extend(wiring.operator_args.iter().cloned());
    args.extend(
        user_args
            .iter()
            .filter(|arg| *arg != "--no-remote-control")
            .cloned(),
    );
    let env = match wiring.sidecar {
        Some(proxy) => proxy.variables(),
        None => BTreeMap::from([(
            "ANTHROPIC_BASE_URL".to_string(),
            format!("http://{}", wiring.proxy_addr),
        )]),
    };
    ClaudeInvocation { env, args, session }
}

fn carries_session(args: &[String]) -> bool {
    args.iter().any(|a| {
        a == "--session-id"
            || a.starts_with("--session-id=")
            || a == "--resume"
            || a.starts_with("--resume=")
            || a == "--continue"
    })
}

fn resolve_claude_binary(env_override: Option<String>) -> String {
    env_override
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "claude".to_string())
}

pub(crate) fn claude_binary() -> String {
    resolve_claude_binary(std::env::var("MIKE_CLAUDE_BIN").ok())
}

#[cfg(unix)]
fn exec_claude(invocation: &ClaudeInvocation) -> anyhow::Result<()> {
    use std::os::unix::process::CommandExt;

    let bin = claude_binary();
    let err = std::process::Command::new(&bin)
        .args(&invocation.args)
        .envs(&invocation.env)
        .exec();

    Err(not_found_help(&bin, err))
}

#[cfg(not(unix))]
fn exec_claude(invocation: &ClaudeInvocation) -> anyhow::Result<()> {
    let bin = claude_binary();
    let status = std::process::Command::new(&bin)
        .args(&invocation.args)
        .envs(&invocation.env)
        .status()
        .map_err(|e| not_found_help(&bin, e))?;
    std::process::exit(status.code().unwrap_or(1));
}

fn not_found_help(bin: &str, err: std::io::Error) -> anyhow::Error {
    anyhow::anyhow!(
        "could not launch `{bin}`: {err}\n\
         Claude Code's `claude` binary was not found. Install Claude Code and \
         ensure `claude` is on your PATH, or set MIKE_CLAUDE_BIN=/full/path/to/claude."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn proxy() -> ProxyEnv {
        ProxyEnv {
            https_proxy: "http://127.0.0.1:9987".into(),
            no_proxy: "localhost,127.0.0.1".into(),
            ca_cert: "/home/x/.local/share/proxy-forward/ca-cert.pem".into(),
        }
    }

    fn down(operator_args: &[String]) -> ClaudeWiring<'_> {
        ClaudeWiring {
            proxy_addr: "127.0.0.1:9988",
            sidecar: None,
            operator_args,
        }
    }

    #[test]
    fn invocation_forwards_user_args_in_order_after_the_session_id() {
        let user = strings(&["-p", "hello world", "--model", "opus"]);
        let id = Uuid::nil();
        let inv = build_claude_invocation(&down(&[]), &user, id);
        let mut expected = strings(&["--session-id", &id.to_string()]);
        expected.extend(user.clone());
        assert_eq!(inv.args, expected);
        assert_eq!(inv.session, Some(id));
    }

    #[test]
    fn invocation_sidecar_down_sets_base_url_without_v1_suffix() {
        let inv = build_claude_invocation(&down(&[]), &[], Uuid::nil());
        assert_eq!(
            inv.env,
            BTreeMap::from([(
                "ANTHROPIC_BASE_URL".to_string(),
                "http://127.0.0.1:9988".to_string()
            )])
        );
    }

    #[test]
    fn invocation_sidecar_up_sets_the_proxy_triple_and_no_base_url() {
        let proxy = proxy();
        let wiring = ClaudeWiring {
            proxy_addr: "127.0.0.1:9988",
            sidecar: Some(&proxy),
            operator_args: &[],
        };
        let inv = build_claude_invocation(&wiring, &[], Uuid::nil());
        assert_eq!(
            inv.env.get("HTTPS_PROXY").map(String::as_str),
            Some("http://127.0.0.1:9987")
        );
        assert_eq!(
            inv.env.get("NO_PROXY").map(String::as_str),
            Some("localhost,127.0.0.1")
        );
        assert!(
            inv.env
                .get("NODE_EXTRA_CA_CERTS")
                .is_some_and(|p| p.ends_with("ca-cert.pem"))
        );
        assert!(!inv.env.contains_key("ANTHROPIC_BASE_URL"));
    }

    #[test]
    fn invocation_injects_only_mikes_own_flags_when_operator_args_are_empty() {
        let inv = build_claude_invocation(&down(&[]), &strings(&["--version"]), Uuid::nil());
        assert_eq!(
            inv.args,
            strings(&[
                "--session-id",
                "00000000-0000-0000-0000-000000000000",
                "--version"
            ])
        );
    }

    #[test]
    fn invocation_operator_args_come_before_user_args() {
        let operator = strings(&["--dangerously-skip-permissions", "--chrome"]);
        let inv = build_claude_invocation(&down(&operator), &strings(&["hi"]), Uuid::nil());
        assert_eq!(
            &inv.args[2..],
            strings(&["--dangerously-skip-permissions", "--chrome", "hi"]).as_slice()
        );
    }

    #[test]
    fn invocation_resume_continue_or_explicit_session_id_preassigns_nothing() {
        for user in [
            strings(&["--resume", "abc"]),
            strings(&["--resume=abc"]),
            strings(&["--continue"]),
            strings(&["--session-id", "abc"]),
            strings(&["--session-id=abc"]),
        ] {
            let inv = build_claude_invocation(&down(&[]), &user, Uuid::nil());
            assert_eq!(inv.session, None, "{user:?}");
            assert!(
                !inv.args.iter().any(|a| a == "--session-id")
                    || user[0].starts_with("--session-id")
            );
            assert_eq!(&inv.args[..user.len()], user.as_slice());
        }
    }

    #[test]
    fn invocation_remote_control_present_is_not_duplicated_and_opt_out_strips_it() {
        let present = build_claude_invocation(
            &down(&[]),
            &strings(&["--remote-control", "hi"]),
            Uuid::nil(),
        );
        assert_eq!(
            present
                .args
                .iter()
                .filter(|a| *a == "--remote-control")
                .count(),
            1
        );
        let opted_out = build_claude_invocation(
            &down(&[]),
            &strings(&["--no-remote-control", "hi"]),
            Uuid::nil(),
        );
        assert!(
            !opted_out.args.iter().any(|a| a.contains("remote-control")),
            "{:?}",
            opted_out.args
        );
        assert!(opted_out.args.contains(&"hi".to_string()));
    }

    #[test]
    fn wants_sidecar_chosen_route_wins_over_the_launch_setting() {
        assert!(wants_sidecar(Some(Route::Sidecar), false));
        assert!(!wants_sidecar(Some(Route::Plain), true));
        assert!(wants_sidecar(None, true));
        assert!(!wants_sidecar(None, false));
    }

    #[tokio::test]
    async fn available_local_ca_cannot_implicitly_enable_sidecar() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let ca = dir.path().join("ca.pem");
        std::fs::write(&ca, "test CA")?;
        let mut config = ProxyConfig::for_tests_in_memory();
        config.launch.ca_cert = Some(ca.to_string_lossy().into_owned());
        assert!(sidecar_env(&config, dir.path()).await.is_none());
        let direct = build_claude_invocation(&down(&[]), &[], Uuid::nil());
        assert!(direct.env.contains_key("ANTHROPIC_BASE_URL"));
        assert!(
            !direct
                .args
                .iter()
                .any(|arg| arg.starts_with("--remote-control"))
        );
        Ok(())
    }

    #[test]
    fn claude_binary_defaults_to_path_lookup() {
        assert_eq!(resolve_claude_binary(None), "claude");
        assert_eq!(resolve_claude_binary(Some(String::new())), "claude");
    }

    #[test]
    fn claude_binary_honours_env_override() {
        assert_eq!(
            resolve_claude_binary(Some("/usr/bin/env".to_string())),
            "/usr/bin/env"
        );
    }
}
