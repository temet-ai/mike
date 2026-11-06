mod build;
mod defaults;
mod source;

pub(crate) use build::{BuildStamp, current_build};
pub(crate) use defaults::code_defaults;
pub(crate) use source::{ChangeSource, ChangeVia};

#[cfg(test)]
mod tests;
