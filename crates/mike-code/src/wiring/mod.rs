mod classify;
mod index;
mod names;
mod spec;

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;

use crate::SourceFile;
use index::{Index, References};
pub use spec::{Kind, WiringError, WiringRules};

pub const MAX_WIRING_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WiringFinding {
    pub kind: Kind,
    pub rule: String,
    pub name: String,
    pub path: String,
    pub line: u32,
    pub message: String,
}

impl WiringFinding {
    pub fn fingerprint(&self) -> String {
        format!("{}:{}", self.rule, self.name)
    }
}

pub fn check(rules: &WiringRules, files: &[SourceFile]) -> Vec<WiringFinding> {
    let index = index::build(rules, files);
    let mut findings: Vec<WiringFinding> = index
        .declarations
        .iter()
        .filter_map(|declaration| finding(rules, &index, files, declaration))
        .collect();
    findings.sort_by(|a, b| {
        (a.kind, &a.path, a.line, &a.name).cmp(&(b.kind, &b.path, b.line, &b.name))
    });
    findings
}

pub fn introduced(before: &[WiringFinding], after: &[WiringFinding]) -> Vec<WiringFinding> {
    let mut old: HashMap<String, usize> = HashMap::new();
    for finding in before {
        *old.entry(finding.fingerprint()).or_default() += 1;
    }
    after
        .iter()
        .filter(|finding| match old.get_mut(&finding.fingerprint()) {
            Some(left) if *left > 0 => {
                *left -= 1;
                false
            }
            _ => true,
        })
        .cloned()
        .collect()
}

pub fn counts(findings: &[WiringFinding]) -> BTreeMap<Kind, usize> {
    let mut counted: BTreeMap<Kind, usize> = Kind::ALL.iter().map(|kind| (*kind, 0)).collect();
    for finding in findings {
        *counted.entry(finding.kind).or_default() += 1;
    }
    counted
}

fn finding(
    rules: &WiringRules,
    index: &Index,
    files: &[SourceFile],
    declaration: &index::Declaration,
) -> Option<WiringFinding> {
    let spec = rules.specs.get(declaration.spec)?;
    let references = index.references.get(declaration.spec)?;
    let route = spec.normalize == spec::Normalize::Route;
    if is_wired(declaration, references, route) {
        return None;
    }
    Some(WiringFinding {
        kind: spec.kind,
        rule: spec.id.clone(),
        name: declaration.name.clone(),
        path: files.get(declaration.file)?.path.clone(),
        line: declaration.line,
        message: spec.message.replace("{name}", &declaration.name),
    })
}

fn is_wired(declaration: &index::Declaration, references: &References, route: bool) -> bool {
    let names = std::iter::once(&declaration.name).chain(declaration.aliases.iter());
    for name in names {
        if let Some(found) = references.by_name.get(name)
            && found
                .iter()
                .any(|reference| !declaration.contains(reference))
        {
            return true;
        }
    }
    route
        && references
            .prefixes
            .iter()
            .any(|prefix| declaration.name.starts_with(prefix.as_str()))
}

fn to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
