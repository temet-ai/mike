pub mod backend;
pub mod backend_choice;
pub mod brief;
pub mod channel_link;
pub mod cli;
pub mod codex;
pub mod env;
pub mod error;
pub mod fleet;
pub mod herdr;
pub mod herdr_core;
pub(crate) mod local_root;
pub mod output;
pub(crate) mod process;
pub mod reap;
pub mod request;
pub(crate) mod seat_fact;
pub mod terminal;
pub mod tmux;
pub mod trust;
pub mod worktree;

pub use backend::{
    BackendId, Capabilities, Check, CloseOutcome, Notice, OpenRequest, PaneRequest, Placement,
    ProbeReport, RunSeat, SplitDirection, Surface, TerminalBackend, ValidationReport, backend_for,
};
pub use brief::{BriefMode, DEFAULT_BRIEF_INLINE_MAX, PromptSource, StagedBrief};
pub use env::ProxyEnv;
pub use error::LaunchError;
pub use output::{ChannelCheck, LaunchOutput};
pub use request::{AssembleOptions, LaunchSpec, SplitLaunchArgs, split_launch_args};
pub use trust::{TrustGate, TrustOutcome};
pub use worktree::WorktreeKind;

pub(crate) const TMUX_SESSION_PREFIX: &str = "mike-";
pub(crate) const DEV_CHANNELS_FLAG: &str = "--dangerously-load-development-channels";

pub(crate) use cli::{
    DoctorReport, KillReport, ServerOpenRequest, ServerOpenStart, doctor_for_server,
    kill_for_server, open_for_server,
};
