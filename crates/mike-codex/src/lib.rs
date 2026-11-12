mod error;
mod model;
mod options;
mod policy;
mod rpc;
mod seat;

pub use error::CodexError;
pub use options::{McpBinding, SeatOptions};
pub use policy::{ApprovalPolicy, SandboxMode, ThreadPolicy};
pub use seat::{CodexSeat, TurnOutcome};

#[cfg(test)]
mod protocol_tests;
#[cfg(test)]
mod seat_tests;
#[cfg(test)]
mod test_support;
