//! Facade: `store` as mike_features sees it (crate split, ADR 0002).

#[cfg(any(test, feature = "test-seams"))]
pub use persistence::test_support;

pub use mike_base::store::*;
pub mod persistence;
