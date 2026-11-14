mod import_graph;
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
pub use scan::{Finding, Scan, new_findings, scan};
pub use source_tree::{SourceFile, read_source_tree};

pub const MAX_SCAN_BYTES: usize = 256 * 1024;
