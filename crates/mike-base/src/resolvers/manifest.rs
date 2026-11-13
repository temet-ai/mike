#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;

use regex::Regex;

const INLINE_FILE_CAP: usize = 8;

const RENDER_BUDGET_BYTES: usize = 6 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("rg not found on PATH (needed for candidate discovery): {0}")]
    RgUnavailable(#[source] std::io::Error),

    #[error("rg exited with an error (not just zero matches): {stderr}")]
    RgFailed { stderr: String },

    #[error("failed to read {path}: {source}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestEntry {
    pub file: PathBuf,
    pub one_line_role: String,
    pub key_excerpt: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub entries: Vec<ManifestEntry>,
    pub file_list: Vec<PathBuf>,
    pub truncated_to_file_list: bool,
}

impl Manifest {
    pub fn render(&self) -> String {
        if self.truncated_to_file_list {
            return render_file_list(&self.file_list);
        }

        let full = render_entries(&self.entries);
        if full.len() > RENDER_BUDGET_BYTES {
            let files: Vec<PathBuf> = self.entries.iter().map(|e| e.file.clone()).collect();
            return render_file_list(&files);
        }
        full
    }
}

pub fn build_manifest(keywords: &[String], scope: &Path) -> Result<Manifest, ManifestError> {
    let mut candidates = discover_candidates(keywords, scope)?;
    candidates.sort();

    if candidates.len() > INLINE_FILE_CAP {
        return Ok(Manifest {
            entries: Vec::new(),
            file_list: candidates,
            truncated_to_file_list: true,
        });
    }

    let mut entries = Vec::with_capacity(candidates.len());
    for file in candidates {
        let (one_line_role, key_excerpt) = extract_role_and_excerpt(&file)?;
        entries.push(ManifestEntry {
            file,
            one_line_role,
            key_excerpt,
        });
    }

    Ok(Manifest {
        entries,
        file_list: Vec::new(),
        truncated_to_file_list: false,
    })
}

fn discover_candidates(keywords: &[String], scope: &Path) -> Result<Vec<PathBuf>, ManifestError> {
    let mut cmd = Command::new("rg");
    cmd.arg("--files-with-matches").arg("-i");
    for kw in keywords {
        cmd.arg("-e").arg(kw);
    }
    cmd.arg("--").arg(scope);

    let output = cmd.output().map_err(ManifestError::RgUnavailable)?;

    if !output.status.success() {
        if output.status.code() == Some(1) {
            return Ok(Vec::new());
        }
        return Err(ManifestError::RgFailed {
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(PathBuf::from)
        .collect())
}

static SIGNATURE_RE: LazyLock<Regex> = LazyLock::new(|| {
    match Regex::new(
        r"^\s*(pub(\([^)]*\))?\s+)?(export\s+)?(default\s+)?(async\s+)?(fn|struct|trait|enum|impl|class|interface|function|const)\s+\w+",
    ) {
        Ok(re) => re,
        Err(e) => panic!("invalid static signature pattern: {e}"),
    }
});

fn extract_role_and_excerpt(path: &Path) -> Result<(String, String), ManifestError> {
    let content = std::fs::read_to_string(path).map_err(|source| ManifestError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    let lines: Vec<&str> = content.lines().collect();

    for (idx, line) in lines.iter().enumerate() {
        if SIGNATURE_RE.is_match(line) {
            let role = doc_comment_role(&lines, idx).unwrap_or_else(|| line.trim().to_string());
            let excerpt = key_excerpt(&lines, idx);
            return Ok((role, excerpt));
        }
    }

    let role = lines
        .iter()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim().to_string())
        .unwrap_or_else(|| "empty file".to_string());
    let excerpt = lines.iter().take(3).copied().collect::<Vec<_>>().join("\n");
    Ok((role, excerpt))
}

fn doc_comment_role(lines: &[&str], item_idx: usize) -> Option<String> {
    let mut doc_lines = Vec::new();
    let mut i = item_idx;
    while i > 0 {
        i -= 1;
        let trimmed = lines[i].trim();
        let Some(text) = trimmed
            .strip_prefix("///")
            .or_else(|| trimmed.strip_prefix("//!"))
        else {
            break;
        };
        doc_lines.push(text.trim().to_string());
    }
    if doc_lines.is_empty() {
        return None;
    }
    doc_lines.reverse();
    Some(truncate_to_two_sentences(&doc_lines.join(" ")))
}

fn truncate_to_two_sentences(text: &str) -> String {
    let sentences: Vec<&str> = text
        .split_terminator('.')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .take(2)
        .collect();
    if sentences.is_empty() {
        return text.to_string();
    }
    format!("{}.", sentences.join(". "))
}

fn key_excerpt(lines: &[&str], item_idx: usize) -> String {
    let end = (item_idx + 4).min(lines.len());
    lines[item_idx..end].join("\n")
}

fn render_entries(entries: &[ManifestEntry]) -> String {
    let mut by_dir: BTreeMap<&Path, Vec<&ManifestEntry>> = BTreeMap::new();
    for entry in entries {
        let dir = entry.file.parent().unwrap_or(Path::new("."));
        by_dir.entry(dir).or_default().push(entry);
    }

    let mut out = String::new();
    for (dir, group) in by_dir {
        out.push_str(&format!("## {}\n\n", dir.display()));
        for entry in group {
            out.push_str(&format!(
                "- `{}` — {}\n",
                entry.file.display(),
                entry.one_line_role
            ));
            out.push_str("  ```\n");
            for line in entry.key_excerpt.lines() {
                out.push_str("  ");
                out.push_str(line);
                out.push('\n');
            }
            out.push_str("  ```\n");
        }
        out.push('\n');
    }
    out
}

fn render_file_list(files: &[PathBuf]) -> String {
    let mut out = String::from(
        "Manifest exceeds the inline cap — file list only, Read specific files for detail:\n\n",
    );
    for file in files {
        out.push_str(&format!("- `{}`\n", file.display()));
    }
    out
}

#[cfg(any(test, feature = "test-seams"))]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test fixtures; tmp-dir I/O and freshly-written files are expected to succeed"
)]
pub mod tests {
    use super::*;
    use anyhow::Context as _;
    use std::fs;
    use tempfile::TempDir;

    fn write_file(dir: &Path, rel: &str, content: &str) -> anyhow::Result<PathBuf> {
        let path = dir.join(rel);
        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("relative path has no parent: {rel}"))?;
        fs::create_dir_all(parent)?;
        fs::write(&path, content)?;
        Ok(path)
    }

    #[test]
    fn build_manifest_finds_only_keyword_matching_files_grouped_by_dir() -> anyhow::Result<()> {
        let tmp = TempDir::new()?;
        write_file(
            tmp.path(),
            "crates/foo/embed_worker.rs",
            "/// Coordinates embedding work for the queue.\npub struct EmbedWorker {\n    pub queue: String,\n}\n",
        )?;
        write_file(tmp.path(), "crates/foo/other.rs", "pub struct Unrelated;\n")?;
        write_file(
            tmp.path(),
            "crates/bar/embed_client.ts",
            "export function embedText(s: string) {\n  return s;\n}\n",
        )?;
        write_file(
            tmp.path(),
            "crates/bar/unrelated.ts",
            "export const x = 1;\n",
        )?;

        let manifest =
            build_manifest(&["embed".to_string()], tmp.path()).context("rg must be on PATH")?;

        assert!(!manifest.truncated_to_file_list);
        assert_eq!(manifest.entries.len(), 2);
        let names: Vec<String> = manifest
            .entries
            .iter()
            .map(|e| {
                e.file
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .ok_or_else(|| {
                        anyhow::anyhow!("entry path has no file name: {}", e.file.display())
                    })
            })
            .collect::<anyhow::Result<_>>()?;
        assert!(names.contains(&"embed_worker.rs".to_string()));
        assert!(names.contains(&"embed_client.ts".to_string()));
        Ok(())
    }

    #[test]
    fn extract_role_and_excerpt_pulls_doc_comment_and_signature() -> anyhow::Result<()> {
        let tmp = TempDir::new()?;
        let path = write_file(
            tmp.path(),
            "embed_worker.rs",
            "/// Coordinates embedding work for the queue.\n/// Retries on transient failures.\npub struct EmbedWorker {\n    pub queue: String,\n}\n",
        )?;

        let (role, excerpt) = extract_role_and_excerpt(&path)?;

        assert_eq!(
            role,
            "Coordinates embedding work for the queue. Retries on transient failures."
        );
        assert!(excerpt.contains("pub struct EmbedWorker"));
        assert!(excerpt.contains("pub queue: String,"));
        Ok(())
    }

    #[test]
    fn build_manifest_beyond_inline_cap_falls_back_to_file_list() -> anyhow::Result<()> {
        let tmp = TempDir::new()?;
        for i in 0..9 {
            write_file(
                tmp.path(),
                &format!("widget_{i}.rs"),
                "pub struct Widget;\n",
            )?;
        }

        let manifest = build_manifest(&["widget".to_string()], tmp.path())?;

        assert!(manifest.truncated_to_file_list);
        assert!(manifest.entries.is_empty());
        assert_eq!(manifest.file_list.len(), 9);

        let rendered = manifest.render();
        assert!(rendered.contains("file list only"));
        Ok(())
    }

    #[test]
    fn render_degrades_to_file_list_when_over_size_budget_even_under_cap() -> anyhow::Result<()> {
        let tmp = TempDir::new()?;
        let huge_doc: String = "/// ".to_string() + &"filler ".repeat(2000) + "\n";
        write_file(
            tmp.path(),
            "oversized.rs",
            &format!("{huge_doc}pub struct Big;\n"),
        )?;

        let manifest = build_manifest(&["big".to_string()], tmp.path())?;

        assert!(!manifest.truncated_to_file_list);
        assert_eq!(manifest.entries.len(), 1);

        let rendered = manifest.render();
        assert!(rendered.contains("file list only"));
        assert!(rendered.len() <= RENDER_BUDGET_BYTES + 512);
        Ok(())
    }

    #[test]
    fn build_manifest_ordering_is_deterministic_across_calls() -> anyhow::Result<()> {
        let tmp = TempDir::new()?;
        for name in ["zebra_embed.rs", "alpha_embed.rs", "mid_embed.rs"] {
            write_file(tmp.path(), name, "pub struct Marker;\n")?;
        }

        let first = build_manifest(&["embed".to_string()], tmp.path())?;
        let second = build_manifest(&["embed".to_string()], tmp.path())?;

        let first_files: Vec<_> = first.entries.iter().map(|e| e.file.clone()).collect();
        let second_files: Vec<_> = second.entries.iter().map(|e| e.file.clone()).collect();
        assert_eq!(first_files, second_files);

        let mut sorted = first_files.clone();
        sorted.sort();
        assert_eq!(first_files, sorted);
        Ok(())
    }

    #[test]
    fn build_manifest_no_matches_returns_empty_manifest() -> anyhow::Result<()> {
        let tmp = TempDir::new()?;
        write_file(tmp.path(), "nothing_here.rs", "pub struct Nope;\n")?;

        let manifest = build_manifest(&["nonexistent_keyword_xyz".to_string()], tmp.path())?;

        assert!(!manifest.truncated_to_file_list);
        assert!(manifest.entries.is_empty());
        Ok(())
    }
}
