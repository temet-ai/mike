mod contract;
mod fleet;
mod model;
mod options;
mod plan;
mod register;
mod rpc;
mod seat;

pub use contract::{SeatKind, render_contract};
pub use fleet::{
    SeatLaunch, begin as begin_fleet_seat, brief_seat, call_seat, identity as seat_identity,
    interrupt_seat, live_seat, stop_seat,
};
pub use model::{CODEX_EFFORTS, agent_problem, effort_problem, model_problem};
pub use options::{McpBinding, SeatOptions, seat_codex_home};
#[cfg(test)]
pub(crate) use options::{operator_codex_home, prepare_home};
pub use plan::{begin_planned, seat_options};
pub use register::{CODEX_BACKEND, CODEX_BACKEND_REASON, bind_seat_thread, namespace_for};
pub use seat::{CodexSeat, TurnOutcome};

#[derive(Debug, thiserror::Error)]
pub enum CodexError {
    #[error("could not start `{program} app-server`: {error}")]
    Spawn { program: String, error: String },
    #[error("the Codex app-server closed its {0}")]
    Closed(&'static str),
    #[error("the Codex app-server did not answer {0} within the call deadline")]
    Timeout(String),
    #[error("the Codex app-server refused {method} with params {params}: {error}")]
    Rpc {
        method: String,
        params: String,
        error: String,
    },
    #[error("the Codex app-server answered {method} without {field}: {answer}")]
    Shape {
        method: String,
        field: &'static str,
        answer: String,
    },
    #[error("could not stop the Codex app-server: {0}")]
    Stop(String),
    #[error("could not prepare the Codex home {path}: {error}")]
    Home { path: String, error: String },
    #[error("turn {turn_id} of thread {thread_id} did not complete before the deadline")]
    TurnDeadline { thread_id: String, turn_id: String },
    #[error("{0}")]
    Record(String),
}

#[cfg(test)]
#[path = "codex_tests.rs"]
mod codex_tests;
#[cfg(test)]
#[path = "live_tests.rs"]
mod live_tests;
