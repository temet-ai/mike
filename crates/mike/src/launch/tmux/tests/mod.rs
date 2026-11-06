#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]

mod close_tests;
mod fake;
mod live;
mod open_tests;
mod probe_tests;
mod validate_tests;

pub(super) use fake::FakeTmux;
