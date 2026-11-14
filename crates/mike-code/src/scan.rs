use std::collections::HashMap;

use ast_grep_config::{RuleConfig, Severity};
use ast_grep_core::NodeMatch;
use ast_grep_core::tree_sitter::{LanguageExt, StrDoc};
use ast_grep_language::SupportLang;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::MAX_SCAN_BYTES;
use crate::language::{language_for, language_name};
use crate::rules::RuleSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    pub rule_id: String,
    pub source: &'static str,
    pub severity: &'static str,
    pub message: String,
    pub line: u32,
    pub column: u32,
    pub matched: String,
    pub fix: Option<String>,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Scan {
    pub language: Option<String>,
    pub rules_run: usize,
    pub skipped: Option<&'static str>,
    pub findings: Vec<Finding>,
}

impl Scan {
    fn skipped(reason: &'static str) -> Scan {
        Scan {
            language: None,
            rules_run: 0,
            skipped: Some(reason),
            findings: Vec::new(),
        }
    }
}

pub fn scan(rules: &RuleSet, path: &str, bytes: &[u8]) -> Scan {
    if bytes.len() > MAX_SCAN_BYTES {
        return Scan::skipped("too_large");
    }
    let Some(language) = language_for(path, bytes) else {
        return Scan::skipped("no_grammar");
    };
    let Ok(source) = std::str::from_utf8(bytes) else {
        return Scan::skipped("not_utf8");
    };
    let applicable = rules.for_file(path, language);
    let root = language.ast_grep(source);
    let mut findings = Vec::new();
    for rule in &applicable {
        for node in root.root().find_all(&rule.matcher) {
            findings.push(finding(rules, rule, &node));
        }
    }
    findings.sort_by(|a, b| (a.line, a.column, &a.rule_id).cmp(&(b.line, b.column, &b.rule_id)));
    Scan {
        language: Some(language_name(language)),
        rules_run: applicable.len(),
        skipped: None,
        findings,
    }
}

pub fn new_findings(before: &[Finding], after: &[Finding]) -> Vec<Finding> {
    let mut old: HashMap<&str, usize> = HashMap::new();
    for finding in before {
        *old.entry(finding.fingerprint.as_str()).or_default() += 1;
    }
    after
        .iter()
        .filter(|finding| match old.get_mut(finding.fingerprint.as_str()) {
            Some(left) if *left > 0 => {
                *left -= 1;
                false
            }
            _ => true,
        })
        .cloned()
        .collect()
}

fn finding(
    rules: &RuleSet,
    rule: &RuleConfig<SupportLang>,
    node: &NodeMatch<'_, StrDoc<SupportLang>>,
) -> Finding {
    let matched = node.text().to_string();
    let start = node.start_pos();
    Finding {
        rule_id: rule.id.clone(),
        source: rules.source_of(&rule.id),
        severity: severity_name(&rule.severity),
        message: rule.get_message(node),
        line: to_u32(start.line() + 1),
        column: to_u32(start.column(node) + 1),
        fix: fix_text(rule, node),
        fingerprint: fingerprint(&rule.id, &matched),
        matched,
    }
}

fn fix_text(
    rule: &RuleConfig<SupportLang>,
    node: &NodeMatch<'_, StrDoc<SupportLang>>,
) -> Option<String> {
    let fixer = rule.get_fixer().ok()?.into_iter().next()?;
    let edit = node.replace_by(&fixer);
    String::from_utf8(edit.inserted_text).ok()
}

pub fn fingerprint(rule_id: &str, matched: &str) -> String {
    let squeezed: String = matched.chars().filter(|c| !c.is_whitespace()).collect();
    let mut digest = Sha256::new();
    digest.update(rule_id.as_bytes());
    digest.update([0]);
    digest.update(squeezed.as_bytes());
    hex::encode(&digest.finalize()[..12])
}

fn to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

pub(crate) fn severity_name(severity: &Severity) -> &'static str {
    match severity {
        Severity::Off => "off",
        Severity::Hint => "hint",
        Severity::Info => "info",
        Severity::Warning => "warning",
        Severity::Error => "error",
    }
}
