pub mod inject;
pub mod stage;
pub mod strict;
pub mod strip;

#[cfg(test)]
pub mod inject_tests;
#[cfg(test)]
pub mod strict_tests;
#[cfg(test)]
pub mod strip_tests;

pub const FIELD: &str = "context_focus_question";
pub const STAGE: &str = "context_focus_question";
