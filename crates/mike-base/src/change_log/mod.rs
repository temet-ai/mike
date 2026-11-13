pub mod build;
pub mod defaults;
pub mod source;

pub use build::{BuildStamp, current_build};
pub use defaults::code_defaults;
pub use source::{ChangeSource, ChangeVia};

#[cfg(test)]
mod tests;
