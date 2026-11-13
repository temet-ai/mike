#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test fixtures; tmp-dir I/O and rg availability are expected to succeed"
)]

use super::*;
use anyhow::Context as _;
use std::fs;
use tempfile::TempDir;

fn write_file(dir: &TempDir, rel: &str, content: &str) -> anyhow::Result<PathBuf> {
    let path = dir.path().join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, content)?;
    Ok(path)
}

#[test]
fn exclusion_actually_applied() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    write_file(&dir, "src/lib.rs", "let needle = 1;\n")?;
    write_file(&dir, "target/debug/build_artifact.rs", "let needle = 2;\n")?;
    write_file(&dir, "node_modules/pkg/index.js", "const needle = 3;\n")?;

    let query = LiteralQuery::new("needle", dir.path());
    let answer = resolve(&query).context("rg should run")?;

    assert_eq!(
        answer.total_hits, 1,
        "target/ and node_modules/ hits must be excluded"
    );
    assert!(answer.groups[0].file.ends_with("src/lib.rs"));
    Ok(())
}

#[test]
fn grouping_by_file() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    write_file(&dir, "a.rs", "needle one\nsome other line\nneedle two\n")?;
    write_file(&dir, "b.rs", "needle three\n")?;

    let query = LiteralQuery::new("needle", dir.path());
    let answer = resolve(&query).context("rg should run")?;

    assert_eq!(
        answer.groups.len(),
        2,
        "hits from two files should form two groups"
    );
    let total_hits_in_groups: usize = answer.groups.iter().map(|g| g.hits.len()).sum();
    assert_eq!(total_hits_in_groups, answer.total_hits);
    Ok(())
}

#[test]
fn pagination_boundary() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let mut content = String::new();
    for i in 0..60 {
        content.push_str(&format!("needle line {i}\n"));
    }
    write_file(&dir, "many.rs", &content)?;

    let mut query = LiteralQuery::new("needle", dir.path());
    let answer = resolve(&query).context("rg should run")?;

    assert_eq!(answer.total_hits, 60);
    let page0_hits: usize = answer.groups.iter().map(|g| g.hits.len()).sum();
    assert_eq!(page0_hits, PAGE_SIZE, "first page must cap at PAGE_SIZE");
    assert!(answer.has_more());

    query.page = 1;
    let answer2 = resolve(&query).context("rg should run")?;
    let page1_hits: usize = answer2.groups.iter().map(|g| g.hits.len()).sum();
    assert_eq!(page1_hits, 10, "second page holds the remaining 10 hits");
    assert!(!answer2.has_more());
    Ok(())
}

#[test]
fn kind_tagging_state_declaration_and_markup() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    write_file(&dir, "state.rs", "let needle = compute();\n")?;
    write_file(&dir, "template.html", "<div class=\"needle\">hi</div>\n")?;

    let query = LiteralQuery::new("needle", dir.path());
    let answer = resolve(&query).context("rg should run")?;

    let all_hits: Vec<&Hit> = answer.groups.iter().flat_map(|g| &g.hits).collect();
    assert!(
        all_hits.iter().any(|h| h.kind == HitKind::StateDeclaration),
        "expected a state-declaration hit"
    );
    assert!(
        all_hits.iter().any(|h| h.kind == HitKind::MarkupUsage),
        "expected a markup-usage hit"
    );
    Ok(())
}

#[test]
fn render_stays_within_size_budget() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let mut content = String::new();
    for i in 0..60 {
        content.push_str(&format!(
            "needle very long line number {i} with extra padding text to grow the render\n"
        ));
    }
    write_file(&dir, "big.rs", &content)?;

    let query = LiteralQuery::new("needle", dir.path());
    let answer = resolve(&query).context("rg should run")?;
    let rendered = answer.render();

    assert!(
        rendered.len() <= RENDER_BYTE_BUDGET + 512,
        "rendered output ({} bytes) must stay near the size budget",
        rendered.len()
    );
    Ok(())
}

#[test]
fn rg_failed_error_carries_stderr() {
    let err = LiteralResolverError::RgFailed {
        status: 2,
        stderr: "boom".to_string(),
    };
    assert!(err.to_string().contains("boom"));
}
