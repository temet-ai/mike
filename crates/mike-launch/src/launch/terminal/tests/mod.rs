#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]

pub mod close_tests;
pub mod fake;
pub mod live;
pub mod open_tests;
pub mod probe_tests;
pub mod validate_tests;

pub use fake::{FakeTerminalHost, fast_timeouts};
