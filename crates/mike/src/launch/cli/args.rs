use clap::{Args, Subcommand};
use uuid::Uuid;

#[derive(Debug, Clone, Args)]
pub struct SpawnArgs {
    #[command(subcommand)]
    pub command: SpawnCommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum SpawnCommand {
    Open(OpenArgs),
    Kill(KillArgs),
    List(ListArgs),
    Doctor(DoctorArgs),
    Warm(WarmArgs),
    Fork(ForkArgs),
    Templates(TemplatesArgs),
    Reap(ReapArgs),
}

#[derive(Debug, Clone, Args)]
#[command(
    override_usage = "mike spawn open <folder> [--tab|--split <l|r|u|d>|--panel] [--json] \
                      [--run <id>] [--seat <name>] [--role <role>] [--instructions <text>] \
                      [--goal <text>] [--session-name <name>] [--prompt-file <path|->] \
                      [--worktree-ok] [--trust|--no-trust] [--no-proxy-env] \
                      [--no-remote-control] [claude args...] [-- <claude args...>]",
    after_help = "The folder comes first. Spawn's own flags may appear anywhere after it; every \
                  other token is a claude argument, kept in order, and a literal `--` forwards \
                  the rest verbatim. Claude receives `--session-id <uuid>`, the operator's \
                  launch_claude_args, your arguments, then `--remote-control` and the prompt \
                  from --prompt-file. --role/--instructions/--goal seed the spawned session's \
                  identity on the launch ledger; --session-name sets its roster name and \
                  defaults to --seat. A value passed explicitly at registration still wins over \
                  this seed. With --json exactly one JSON object is printed on stdout. \
                  Exit codes: 0 ok, 1 refused or failed (including a channel-check timeout), \
                  69 no backend chosen, 127 a required executable is missing."
)]
pub struct OpenArgs {
    #[arg(
        required = true,
        trailing_var_arg = true,
        allow_hyphen_values = true,
        num_args = 1..,
        value_name = "FOLDER_AND_ARGS"
    )]
    pub tokens: Vec<String>,
}

#[derive(Debug, Clone, Args)]
pub struct KillArgs {
    #[arg(long)]
    pub session: Uuid,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Args)]
pub struct ListArgs {
    #[arg(long)]
    pub run: Option<String>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Args)]
pub struct DoctorArgs {
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Args)]
#[command(
    override_usage = "mike spawn warm <folder> --name <discipline> [placement] [--json] \
                      [--role <role>] [--instructions <text>] [--goal <text>] \
                      [--session-name <name>] [claude args...]",
    after_help = "Launches exactly like `open`, then records the claude arguments as a named \
                  template. A second warm under the same name supersedes the previous one and \
                  prints the session id it replaced on stderr. --name is the template's \
                  discipline, not the session's roster name; when --role is absent it defaults \
                  the seeded role to that discipline."
)]
pub struct WarmArgs {
    #[arg(
        required = true,
        trailing_var_arg = true,
        allow_hyphen_values = true,
        num_args = 1..,
        value_name = "FOLDER_AND_ARGS"
    )]
    pub tokens: Vec<String>,
}

#[derive(Debug, Clone, Args)]
#[command(
    override_usage = "mike spawn fork <discipline|session-uuid> [--folder <dir>] [placement] \
                      [--json] [--worktree-ok] [--prompt-file <path|->] [--allow-prefix-change] \
                      [--role <role>] [--instructions <text>] [--goal <text>] \
                      [--session-name <name>] [claude args...]",
    after_help = "Prepends --resume <template-session> --fork-session --session-id <new> and the \
                  template's recorded claude arguments ahead of your own. Refuses a --model, \
                  --agent, --append-system-prompt-file, --settings, --add-dir or --mcp-config \
                  that disagrees with what the template recorded, unless --allow-prefix-change \
                  is given. --role/--instructions/--goal/--session-name seed identity on the \
                  forked session exactly as they do for `open`."
)]
pub struct ForkArgs {
    #[arg(
        required = true,
        trailing_var_arg = true,
        allow_hyphen_values = true,
        num_args = 1..,
        value_name = "TEMPLATE_AND_ARGS"
    )]
    pub tokens: Vec<String>,
}

#[derive(Debug, Clone, Args)]
pub struct TemplatesArgs {
    #[command(subcommand)]
    pub command: Option<TemplatesCommand>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Subcommand)]
pub enum TemplatesCommand {
    Show(TemplatesShowArgs),
    Delete(TemplatesDeleteArgs),
}

#[derive(Debug, Clone, Args)]
pub struct TemplatesShowArgs {
    pub name: String,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Args)]
pub struct TemplatesDeleteArgs {
    pub name: String,
    #[arg(long)]
    pub yes: bool,
}

#[derive(Debug, Clone, Args)]
pub struct ReapArgs {
    #[arg(long)]
    pub yes: bool,
    #[arg(long)]
    pub json: bool,
}
