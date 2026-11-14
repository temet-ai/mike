pub use mike_launch::store::persistence::mailbox::*;
pub mod channel;
pub mod fleet_seat;
pub mod fleet_target;
#[cfg(test)]
pub mod tests;
pub use fleet_target::{FleetTarget, TargetError};
