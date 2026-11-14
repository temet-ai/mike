use anyhow::{Context, Result};

use super::{Poster, fill, scan_change};
use crate::change_record::watch::Git;
use crate::store::persistence::change_record::code_scan::{NewCodeScan, PendingChange};

async fn repo_with(files: &[(&str, &str)]) -> Result<(tempfile::TempDir, Git, Vec<String>)> {
    let dir = tempfile::tempdir()?;
    let root = dir.path().to_str().context("path")?.to_string();
    let git = Git::new(root.clone());
    git.run(&["init", "-q"]).await?;
    let mut shas = Vec::new();
    for (name, text) in files {
        std::fs::write(dir.path().join(name), text)?;
        let out = git.run(&["hash-object", "-w", name]).await?;
        shas.push(String::from_utf8(out)?.trim().to_string());
    }
    Ok((dir, git, shas))
}

fn rules() -> Result<&'static mike_code::RuleSet> {
    mike_code::RuleSet::builtin().map_err(|err| anyhow::anyhow!("{err}"))
}

fn active() -> Result<super::ActiveRules> {
    Ok(super::ActiveRules::Builtin(rules()?))
}

#[tokio::test]
async fn an_edit_records_only_the_findings_it_added() -> Result<()> {
    let (_dir, git, shas) = repo_with(&[
        ("old.rs", "fn a() { unsafe { f() } }\n"),
        (
            "new.rs",
            concat!(
                "fn a() { unsafe { f() } }\nfn b(err: E) { tracing::warn!(%e",
                "rr, \"x\"); }\n"
            ),
        ),
    ])
    .await?;
    let change = PendingChange {
        id: 7,
        path: "src/lib.rs".to_string(),
        before_sha: Some(shas[0].clone()),
        after_sha: Some(shas[1].clone()),
    };
    let record = scan_change(&git, active()?, git.root(), &change).await;
    assert_eq!(record.error, None);
    assert_eq!(record.language.as_deref(), Some("rust"));
    assert_eq!((record.findings_before, record.findings_after), (1, 2));
    let added: Vec<&str> = record.added.iter().map(|f| f.rule_id.as_str()).collect();
    assert_eq!(added, vec!["rust-log-field-names"]);
    assert_eq!(record.present.len(), 2);
    Ok(())
}

#[tokio::test]
async fn a_deleted_file_and_an_unknown_blob_are_recorded_not_dropped() -> Result<()> {
    let (_dir, git, _) = repo_with(&[]).await?;
    let deleted = PendingChange {
        id: 1,
        path: "a.rs".to_string(),
        before_sha: Some("abc".to_string()),
        after_sha: None,
    };
    let record = scan_change(&git, active()?, git.root(), &deleted).await;
    assert_eq!(record.skipped.as_deref(), Some("deleted"));
    let missing = PendingChange {
        after_sha: Some("0123456789abcdef0123456789abcdef01234567".to_string()),
        ..deleted
    };
    let record = scan_change(&git, active()?, git.root(), &missing).await;
    assert!(record.error.is_some());
    Ok(())
}

#[test]
fn a_file_without_a_grammar_is_skipped_with_its_reason() -> Result<()> {
    let rules = rules()?;
    let old = mike_code::scan(rules, "page.svelte", b"<p/>");
    let new = mike_code::scan(rules, "page.svelte", b"<p>{@html x}</p>");
    let mut record = NewCodeScan::default();
    fill(&mut record, &old, &new);
    assert_eq!(record.skipped.as_deref(), Some("no_grammar"));
    assert!(record.added.is_empty());
    Ok(())
}

fn ready(
    id: i64,
    session: uuid::Uuid,
) -> crate::store::persistence::change_record::code_scan::ReadyFinding {
    crate::store::persistence::change_record::code_scan::ReadyFinding {
        id,
        session,
        path: "a.rs".to_string(),
        line: 1,
        rule_id: "rust-unsafe".to_string(),
        message: "m".to_string(),
        fix: None,
    }
}

#[test]
fn findings_wait_below_an_unnamed_change_and_are_posted_once() {
    let notes = std::sync::Arc::new(mike_base::code_notes::CodeNotes::default());
    let mut poster = Poster::new(notes.clone());
    let session = uuid::Uuid::new_v4();
    poster.hand_over(vec![ready(5, session), ready(9, session)], Some(7));
    assert_eq!(notes.take(session, 10).len(), 2);
    poster.hand_over(vec![ready(9, session), ready(8, session)], None);
    let again: Vec<i64> = notes.take(session, 10).iter().map(|n| n.id).collect();
    assert_eq!(again, vec![8], "9 was already posted; 8 was named later");
    poster.hand_over(Vec::new(), None);
    assert!(notes.take(session, 10).is_empty());
}

const NO_PRINTLN: &str = "id: no-println\nlanguage: rust\nseverity: warning\nrule:\n  pattern: println!($$$)\nmessage: Use tracing.\n";

#[tokio::test]
async fn project_rules_load_from_sgconfig_and_mike_dir_and_reload_when_changed() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let root = dir.path().to_str().context("path")?.to_string();
    std::fs::create_dir_all(dir.path().join("lint"))?;
    std::fs::write(dir.path().join("sgconfig.yml"), "ruleDirs:\n  - lint\n")?;
    std::fs::write(dir.path().join("lint/no-println.yml"), NO_PRINTLN)?;
    let mut project = super::ProjectRules::new(rules()?);
    let active = project.for_checkout(&root).await;
    assert!(matches!(active, super::ActiveRules::Project(_)));
    let found = mike_code::scan(active.get(), "src/a.rs", b"fn a() { println!(\"x\"); }\n");
    assert!(
        found
            .findings
            .iter()
            .any(|f| f.rule_id == "no-println" && f.source == "project")
    );
    let status = super::project_rules_status();
    let mine = status
        .iter()
        .find(|s| s.checkout == root)
        .context("no status")?;
    assert_eq!(mine.files, vec!["lint/no-println.yml".to_string()]);
    assert_eq!(mine.rules, vec!["no-println".to_string()]);
    Ok(())
}

#[tokio::test]
async fn a_broken_project_rule_keeps_the_builtin_rules_and_reports_the_file() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let root = dir.path().to_str().context("path")?.to_string();
    std::fs::create_dir_all(dir.path().join(super::project_rules::MIKE_RULE_DIR))?;
    std::fs::write(
        dir.path()
            .join(super::project_rules::MIKE_RULE_DIR)
            .join("bad.yml"),
        "id: x\nrule: [",
    )?;
    let mut project = super::ProjectRules::new(rules()?);
    let active = project.for_checkout(&root).await;
    assert!(matches!(active, super::ActiveRules::Builtin(_)));
    let status = super::project_rules_status();
    let mine = status
        .iter()
        .find(|s| s.checkout == root)
        .context("no status")?;
    assert!(mine.error.as_deref().is_some_and(|e| e.contains("bad.yml")));
    Ok(())
}

#[tokio::test]
async fn a_checkout_without_rule_files_uses_the_builtin_rules() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let root = dir.path().to_str().context("path")?.to_string();
    let mut project = super::ProjectRules::new(rules()?);
    assert!(matches!(
        project.for_checkout(&root).await,
        super::ActiveRules::Builtin(_)
    ));
    assert!(
        super::project_rules_status()
            .iter()
            .all(|s| s.checkout != root)
    );
    Ok(())
}
