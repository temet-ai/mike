mod import_graph;
mod keep;
mod language;
mod reachability;
mod rules;
mod scan;
mod source_tree;
pub mod wiring;

#[cfg(test)]
mod tests;

pub use import_graph::{ImportGraph, build_import_graph};
pub use language::{grammars, language_for, language_name};
pub use reachability::{UnreachableFinding, check_reachability};
pub use rules::{RuleError, RuleInfo, RuleSet, SOURCE_BUILTIN, SOURCE_PROJECT, sgconfig_rule_dirs};
pub use keep::{MARKER as KEEP_MARKER, MIN_WORDS as KEEP_MIN_WORDS};
pub use scan::{Finding, Kept, Scan, new_findings, scan};
pub use source_tree::{SourceFile, read_source_tree};

pub const MAX_SCAN_BYTES: usize = 256 * 1024;
