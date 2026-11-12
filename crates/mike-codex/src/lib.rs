mod error;
mod events;
mod model;
mod observation;
pub mod observe;
mod options;
mod policy;
pub mod queue;
pub mod responses;
pub mod responses_body;
mod rpc;
mod rpc_frame;
mod seat;

pub use error::CodexError;
pub use observation::{
    ItemUpdate, NotificationObservation, SessionPhase, SessionUpdate, ThreadUsage,
    observe_notification,
};
pub use options::{McpBinding, SeatOptions};
pub use policy::{ApprovalPolicy, SandboxMode, ThreadPolicy};
pub use seat::{CodexSeat, TurnOutcome};

#[cfg(test)]
mod protocol_tests;
#[cfg(test)]
mod seat_tests;
#[cfg(test)]
mod test_support;
