pub(crate) mod inject;
pub(crate) mod stage;
pub(crate) mod strict;
pub(crate) mod strip;

#[cfg(test)]
mod inject_tests;
#[cfg(test)]
mod strict_tests;
#[cfg(test)]
mod strip_tests;

pub(crate) const FIELD: &str = "context_focus_question";
pub(crate) const STAGE: &str = "context_focus_question";
