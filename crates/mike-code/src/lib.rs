mod language;
mod rules;
mod scan;

#[cfg(test)]
mod tests;

pub use language::language_for;
pub use rules::{RuleError, RuleSet};
pub use scan::{Finding, Scan, new_findings, scan};

pub const MAX_SCAN_BYTES: usize = 256 * 1024;
