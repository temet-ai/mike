pub mod contract;
pub mod fleet;
pub mod model;
pub mod observation;
pub mod options;
pub mod register;

pub use contract::{SeatKind, render_contract};
pub use fleet::{
    SeatLaunch, begin as begin_fleet_seat, brief_seat, call_seat, identity as seat_identity,
    interrupt_seat, live_seat, stop_seat,
};
pub use mike_codex::{CodexError, CodexSeat, McpBinding, SeatOptions, TurnOutcome};
pub use model::{CODEX_EFFORTS, agent_problem, effort_problem, model_problem};
pub use options::seat_codex_home;
#[cfg(test)]
pub use options::{operator_codex_home, prepare_home, seat_policy};
pub use register::{CODEX_BACKEND, CODEX_BACKEND_REASON, bind_seat_thread, namespace_for};

#[cfg(test)]
#[path = "codex_tests.rs"]
pub mod codex_tests;
#[cfg(test)]
#[path = "live_tests.rs"]
pub mod live_tests;
