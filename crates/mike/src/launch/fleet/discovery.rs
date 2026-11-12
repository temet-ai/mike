use std::path::{Path, PathBuf};

use clap::Args;
use serde::Serialize;

use super::locate::{Definition, Located, locate, parse};
use super::watch::git;

#[derive(Debug, Clone, Args)]
pub struct DiscoveryArgs {
    #[arg(long, default_value = ".")]
    pub repo: PathBuf,
    #[arg(long, required_unless_present = "path")]
    pub symbol: Option<String>,
    #[arg(long)]
    pub path: Option<String>,
    #[arg(long, default_value_t = 8, value_parser = clap::value_parser!(u16).range(0..=40))]
    pub context_lines: u16,
}

#[derive(Debug, Serialize)]
pub struct SourceContext {
    pub full_file_path: String,
    pub source_sha256: String,
    pub first_line: usize,
    pub lines: Vec<String>,
    pub total_lines: usize,
}

#[derive(Debug, Serialize)]
pub struct Discovery {
    #[serde(flatten)]
    pub located: Located,
    pub repository: String,
    pub commit: Option<String>,
    pub dirty: bool,
    pub textual_references: Vec<Definition>,
    pub references_truncated: bool,
    pub source_context: Vec<SourceContext>,
    pub context_definitions_omitted: usize,
    pub errors: Vec<String>,
    pub semantics: &'static str,
}

async fn source_context(
    root: &Path,
    definition: &Definition,
    radius: u16,
) -> anyhow::Result<SourceContext> {
    let path = tokio::fs::canonicalize(root.join(&definition.path)).await?;
    anyhow::ensure!(path.starts_with(root), "source path escapes repository");
    let bytes = tokio::fs::read(&path).await?;
    let text = std::str::from_utf8(&bytes)?;
    let all: Vec<_> = text.lines().collect();
    let at = definition.line.saturating_sub(1) as usize;
    anyhow::ensure!(at < all.len(), "source changed since definition lookup");
    let start = at.saturating_sub(usize::from(radius));
    let end = (at + usize::from(radius) + 1).min(all.len());
    Ok(SourceContext {
        full_file_path: path.display().to_string(),
        source_sha256: crate::wire::manifest::sha256_hex(&bytes),
        first_line: start + 1,
        lines: all[start..end]
            .iter()
            .map(|line| (*line).to_string())
            .collect(),
        total_lines: all.len(),
    })
}

pub async fn discover(args: &DiscoveryArgs) -> anyhow::Result<Discovery> {
    let root = tokio::fs::canonicalize(&args.repo).await?;
    let repo = root
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("repository path must be UTF-8"))?;
    let head = git(repo, &["rev-parse", "HEAD"], None)
        .await
        .ok_or_else(|| anyhow::anyhow!("repository is not a readable Git checkout"))?;
    let status = git(repo, &["status", "--porcelain"], None)
        .await
        .ok_or_else(|| anyhow::anyhow!("cannot establish checkout state"))?;
    let located = locate(repo, args.symbol.as_deref(), args.path.as_deref()).await;
    let mut errors = Vec::new();
    let (textual_references, references_truncated) = if let Some(symbol) = &args.symbol {
        let mut command = vec![
            "grep",
            "-n",
            "-I",
            "-w",
            "-F",
            "--untracked",
            "-e",
            symbol,
            "--",
        ];
        if let Some(path) = &args.path {
            command.push(path);
        }
        let raw = git(repo, &command, None).await.unwrap_or_default();
        parse(&String::from_utf8_lossy(&raw))
    } else {
        (Vec::new(), false)
    };
    let mut contexts = Vec::new();
    if args.context_lines > 0 {
        for definition in located.definitions.iter().take(8) {
            match source_context(&root, definition, args.context_lines).await {
                Ok(context) => contexts.push(context),
                Err(error) => errors.push(format!("{}: {error}", definition.path)),
            }
        }
        if located.definitions.is_empty()
            && let Some(path) = &args.path
        {
            let definition = Definition {
                path: path.clone(),
                line: 1,
                text: String::new(),
            };
            match source_context(&root, &definition, args.context_lines).await {
                Ok(context) => contexts.push(context),
                Err(error) => errors.push(format!("{path}: {error}")),
            }
        }
    }
    let omitted = located.definitions.len().saturating_sub(contexts.len());
    Ok(Discovery {
        located,
        repository: repo.to_string(),
        commit: Some(String::from_utf8(head)?.trim().to_string()),
        dirty: !status.is_empty(),
        textual_references,
        references_truncated,
        source_context: contexts,
        context_definitions_omitted: omitted,
        errors,
        semantics: "Live textual definitions/references, not a semantic call graph. Context has whole-file paths and hashes; read those files for complete content. Concurrent edits can change the checkout during this read.",
    })
}

pub async fn run(args: DiscoveryArgs) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(&discover(&args).await?)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn context_is_bounded_and_has_a_full_source_reference() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = tokio::fs::canonicalize(temp.path()).await?;
        tokio::fs::write(
            root.join("main.py"),
            "one\ntwo\ndef load():\n    return 3\nfive\n",
        )
        .await?;
        let definition = Definition {
            path: "main.py".into(),
            line: 3,
            text: "def load():".into(),
        };
        let context = source_context(&root, &definition, 1).await?;
        assert_eq!(context.lines, ["two", "def load():", "    return 3"]);
        assert_eq!(context.total_lines, 5);
        assert_eq!(context.source_sha256.len(), 64);
        assert_eq!(
            context.full_file_path,
            root.join("main.py").display().to_string()
        );
        let missing = Definition {
            path: "missing".into(),
            ..definition
        };
        assert!(source_context(&root, &missing, 1).await.is_err());
        Ok(())
    }
}
