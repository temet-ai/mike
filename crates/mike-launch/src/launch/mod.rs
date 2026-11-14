pub mod backend;
pub mod backend_choice;
pub mod brief;
pub mod brief_delivery;
pub mod channel_link;
pub mod claude_bin;
pub mod cli;
pub mod codex;
pub mod env;
pub mod error;
pub mod fleet;
pub mod herdr;
pub mod herdr_core;
pub mod local_root;
pub mod output;
pub mod process;
pub mod reap;
pub mod request;
pub mod seat;
pub mod seat_fact;
pub mod session_tail;
pub mod stack_starter;
pub mod state;
pub mod terminal;
pub mod tmux;
pub mod trust;
pub mod worktree;

pub use cli::{DoctorReport, KillReport, doctor_for_server, kill_for_server};

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

pub const TMUX_SESSION_PREFIX: &str = "mike-";
pub const DEV_CHANNELS_FLAG: &str = "--dangerously-load-development-channels";

#[cfg(test)]
pub mod cross_crate_tests;
