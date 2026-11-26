use std::time::{Duration, Instant};

use mike_code::{Finding, Scan, new_findings, scan};

use super::inherited::inherited;
use super::project_rules::ActiveRules;

use crate::change_record::watch::Git;
use crate::change_record::watch::git::STORE_LIMIT;
use crate::store::persistence::change_record::code_scan::{NewCodeScan, NewFinding, PendingChange};

const SCAN_TIMEOUT: Duration = Duration::from_secs(2);

pub async fn scan_change(
    git: &Git,
    rules: ActiveRules,
    root: &str,
    change: &PendingChange,
) -> NewCodeScan {
    let mut record = NewCodeScan {
        change_event_id: change.id,
        checkout: root.to_string(),
        path: change.path.clone(),
        rules_version: rules.get().version().to_string(),
        ..NewCodeScan::default()
    };
    let Some(after_sha) = change.after_sha.as_deref() else {
        record.skipped = Some("deleted".to_string());
        return record;
    };
    let after = match git.run(&["cat-file", "blob", after_sha]).await {
        Ok(bytes) => bytes,
        Err(_) if too_large_to_store(git, &change.path) => {
            record.skipped = Some("too_large".to_string());
            return record;
        }
        Err(err) => {
            record.error = Some(err.to_string());
            return record;
        }
    };
    let before = match change.before_sha.as_deref() {
        Some(sha) if sha == after_sha => after.clone(),
        Some(sha) => git
            .run(&["cat-file", "blob", sha])
            .await
            .unwrap_or_default(),
        None => Vec::new(),
    };
    record.bytes = i32::try_from(after.len()).unwrap_or(i32::MAX);
    let path = change.path.clone();
    let started = Instant::now();
    let for_job = rules.clone();
    let job = tokio::task::spawn_blocking(move || {
        let set = for_job.get();
        (scan(set, &path, &before), scan(set, &path, &after))
    });
    match tokio::time::timeout(SCAN_TIMEOUT, job).await {
        Ok(Ok((old, new))) => fill(&mut record, &old, &new),
        Ok(Err(_)) => record.error = Some("the scan panicked".to_string()),
        Err(_) => record.error = Some(format!("the scan took longer than {SCAN_TIMEOUT:?}")),
    }
    drop_inherited(git, rules, &change.path, &mut record).await;
    record.duration_us = i32::try_from(started.elapsed().as_micros()).unwrap_or(i32::MAX);
    record
}

async fn drop_inherited(git: &Git, rules: ActiveRules, path: &str, record: &mut NewCodeScan) {
    if record.added.is_empty() {
        return;
    }
    match inherited(git, rules, path).await {
        Ok(known) => record.added.retain(|f| !known.contains(&f.fingerprint)),
        Err(err) => {
            tracing::warn!(root = %git.root(), path = %path, error = %err, "code scan: what git brought into a file could not be read");
        }
    }
}

fn too_large_to_store(git: &Git, path: &str) -> bool {
    std::fs::symlink_metadata(git.absolute(path)).is_ok_and(|m| m.len() > STORE_LIMIT)
}

pub fn fill(record: &mut NewCodeScan, old: &Scan, new: &Scan) {
    record.language.clone_from(&new.language);
    record.rules_run = i32::try_from(new.rules_run).unwrap_or(i32::MAX);
    record.skipped = new.skipped.map(str::to_string);
    record.findings_before = count(old.findings.len());
    record.findings_after = count(new.findings.len());
    record.present = new.findings.iter().map(|f| f.fingerprint.clone()).collect();
    record.kept = new
        .kept
        .iter()
        .map(|k| (k.finding.fingerprint.clone(), k.reason.clone()))
        .collect();
    let was: Vec<Finding> = old
        .findings
        .iter()
        .chain(old.kept.iter().map(|k| &k.finding))
        .cloned()
        .collect();
    let added = new_findings(&was, &new.findings)
        .into_iter()
        .map(|f| (f, None));
    let kept = new_findings(
        &was,
        &new.kept
            .iter()
            .map(|k| k.finding.clone())
            .collect::<Vec<_>>(),
    )
    .into_iter()
    .map(|f| {
        let reason = new
            .kept
            .iter()
            .find(|k| k.finding.fingerprint == f.fingerprint)
            .map(|k| k.reason.clone());
        (f, reason)
    });
    record.added = added
        .chain(kept)
        .map(|(f, kept)| new_finding(f, kept))
        .collect();
}

fn new_finding(f: Finding, kept: Option<String>) -> NewFinding {
    NewFinding {
        rule_id: f.rule_id,
        rule_source: f.source.to_string(),
        severity: f.severity.to_string(),
        message: f.message,
        line: i32::try_from(f.line).unwrap_or(i32::MAX),
        col: i32::try_from(f.column).unwrap_or(i32::MAX),
        matched: f.matched,
        fix: f.fix,
        fingerprint: f.fingerprint,
        kept,
    }
}

fn count(n: usize) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}
