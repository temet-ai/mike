pub use crate::launch::seat_fact::{SeatRole, seat_role};

#[cfg(any(test, feature = "test-seams"))]
pub use crate::launch::seat_fact::remember_for_test;
