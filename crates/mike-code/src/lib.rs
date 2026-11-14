mod language;
mod rules;
mod scan;

#[cfg(test)]
mod tests;

pub use language::{grammars, language_for, language_name};
pub use rules::{RuleError, RuleInfo, RuleSet, SOURCE_BUILTIN, SOURCE_PROJECT, sgconfig_rule_dirs};
pub use scan::{Finding, Scan, new_findings, scan};

pub const MAX_SCAN_BYTES: usize = 256 * 1024;
