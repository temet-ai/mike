mod immediate;
pub(crate) mod live_backlog;
pub(crate) mod live_judge;
mod live_shared;
mod live_tick;
mod ownership;
mod rejudge;
pub(crate) mod session_end;
mod session_poll;
mod spawn;
#[cfg(any(test, feature = "test-seams"))]
mod spawn_test_seams;
mod turn_sweep;
mod turn_sweep_log;
pub(crate) mod v2_judge;

#[cfg(test)]
mod live_backlog_tests;
#[cfg(test)]
mod ownership_test_model;
#[cfg(test)]
mod ownership_test_support;
#[cfg(test)]
mod ownership_tests;
#[cfg(test)]
mod turn_sweep_tests;

pub use session_end::shadow::spawn_session_end_shadow;
pub use session_poll::spawn_session_judge_worker;
pub use spawn::spawn_judge_worker;

#[cfg(any(test, feature = "test-seams"))]
pub use session_poll::run_session_judge_once;
#[cfg(any(test, feature = "test-seams"))]
pub use spawn_test_seams::{
    default_v2_bundle_components, run_immediate_once, run_judge_once,
    run_judge_sweeps_and_report_breaker_tripped, run_live_judge_once,
};

pub(crate) use session_poll::digest as session_poll_digest;
pub(crate) use session_poll::digest::SESSION_JUDGE_SYSTEM_PROMPT;
pub(crate) use session_poll::resolve_session_judge_runtime;
pub(crate) use turn_sweep::resolve_turn_judge_runtime;
