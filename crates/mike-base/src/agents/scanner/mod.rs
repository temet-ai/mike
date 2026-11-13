use std::path::{Path, PathBuf};

use tokio::fs;

use crate::store::persistence::{PersistError, PersistHandle};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Default)]
pub struct ParsedAgentFile {
    pub name: Option<String>,
    pub description: Option<String>,
    pub persona: Option<String>,
}

pub fn parse_frontmatter(content: &str) -> ParsedAgentFile {
    let trimmed = content.trim_start_matches('\u{feff}');
    let body = match trimmed.strip_prefix("---\n") {
        Some(rest) => rest,
        None => match trimmed.strip_prefix("---\r\n") {
            Some(rest) => rest,
            None => return ParsedAgentFile::default(),
        },
    };
    let end_marker = ["\n---\n", "\n---\r\n"]
        .iter()
        .find_map(|m| body.find(m))
        .unwrap_or(body.len());
    let frontmatter = &body[..end_marker];

    let mut name: Option<String> = None;
    let mut description: Option<String> = None;

    let mut iter = frontmatter.lines().peekable();
    while let Some(line) = iter.next() {
        if line.starts_with(|c: char| c.is_whitespace()) {
            continue;
        }
        if let Some(raw) = line.strip_prefix("name:") {
            name = read_scalar_value(raw, &mut iter);
        } else if let Some(raw) = line.strip_prefix("description:") {
            description = read_scalar_value(raw, &mut iter);
        }
    }

    ParsedAgentFile {
        name,
        description,
        persona: None,
    }
}

fn read_scalar_value<'a, I>(raw: &str, iter: &mut std::iter::Peekable<I>) -> Option<String>
where
    I: Iterator<Item = &'a str>,
{
    let inline = raw.trim();

    enum Mode {
        Folded,
        Literal,
    }
    let (mode, mut parts): (Mode, Vec<String>) = match inline {
        ">" | ">-" | ">+" => (Mode::Folded, Vec::new()),
        "|" | "|-" | "|+" => (Mode::Literal, Vec::new()),
        "" => (Mode::Folded, Vec::new()),
        plain => {
            let stripped = plain.trim_matches(['"', '\'']).to_string();
            let next_is_continuation = iter
                .peek()
                .is_some_and(|l| l.starts_with(|c: char| c.is_whitespace()));
            if !next_is_continuation {
                return non_empty(stripped);
            }
            (Mode::Folded, vec![stripped])
        }
    };

    while let Some(next) = iter.peek() {
        if !next.starts_with(|c: char| c.is_whitespace()) {
            break;
        }
        let Some(line) = iter.next() else { break };
        let trimmed_line = line.trim();
        if trimmed_line.is_empty() {
            if let Mode::Literal = mode {
                parts.push(String::new());
            }
            continue;
        }
        parts.push(trimmed_line.to_string());
    }

    let joined = match mode {
        Mode::Folded => parts.join(" "),
        Mode::Literal => parts.join("\n"),
    };
    non_empty(joined.trim().to_string())
}

fn non_empty(s: String) -> Option<String> {
    if s.is_empty() { None } else { Some(s) }
}

fn split_frontmatter_body(content: &str) -> Option<&str> {
    let trimmed = content.trim_start_matches('\u{feff}');
    let after_open = trimmed
        .strip_prefix("---\n")
        .or_else(|| trimmed.strip_prefix("---\r\n"))?;
    let (idx, marker_len) = ["\n---\n", "\n---\r\n"]
        .iter()
        .find_map(|m| after_open.find(m).map(|i| (i, m.len())))?;
    Some(&after_open[idx + marker_len..])
}

fn parse_body_persona(content: &str) -> Option<String> {
    let body = split_frontmatter_body(content)?;
    crate::extract::request::extract_subagent_persona(body)
}

async fn read_agent_files_in(dir: &Path) -> Vec<(PathBuf, ParsedAgentFile)> {
    let mut out = Vec::new();
    let mut entries = match fs::read_dir(dir).await {
        Ok(e) => e,
        Err(_) => return out,
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let abs = match fs::canonicalize(&path).await {
            Ok(p) => p,
            Err(_) => path.clone(),
        };
        let Ok(content) = fs::read_to_string(&abs).await else {
            continue;
        };
        let mut parsed = parse_frontmatter(&content);
        parsed.persona = parse_body_persona(&content);
        out.push((abs, parsed));
    }
    out
}

pub async fn scan_disk_agents(persist: &PersistHandle) -> Result<usize, PersistError> {
    scan_dirs(persist, &resolve_scan_dirs()).await
}

fn resolve_scan_dirs() -> Vec<PathBuf> {
    let mut targets: Vec<PathBuf> = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        let mut p = PathBuf::from(home);
        p.push(".claude/agents");
        targets.push(p);
    }
    if let Ok(cwd) = std::env::current_dir() {
        let mut p = cwd;
        p.push(".claude/agents");
        targets.push(p);
    }
    targets
}

async fn collect_agent_files(
    targets: &[PathBuf],
) -> (
    Vec<(String, String, Option<String>)>,
    std::collections::BTreeMap<String, String>,
) {
    let mut triples: Vec<(String, String, Option<String>)> = Vec::new();
    let mut personas: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    for dir in targets {
        let files = read_agent_files_in(dir).await;
        for (abs_path, parsed) in files {
            let name = parsed.name.clone().unwrap_or_else(|| {
                abs_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("agent")
                    .to_string()
            });
            let path_str = abs_path.to_string_lossy().to_string();
            if !name.is_empty()
                && let Some(persona) = &parsed.persona
            {
                personas.insert(name.clone(), persona.clone());
            }
            triples.push((name, path_str, parsed.description));
        }
    }
    (triples, personas)
}

async fn upsert_resolved_personas(
    persist: &PersistHandle,
    personas: &std::collections::BTreeMap<String, String>,
    ids: Vec<(String, i64)>,
) -> Result<(), PersistError> {
    for (name, agent_id) in ids {
        if let Some(persona) = personas.get(&name) {
            persist.upsert_persona_alias(agent_id, persona).await?;
        }
    }
    Ok(())
}

fn log_one_alternate_identity(alias: &crate::store::persistence::agents::AgentAlias) {
    tracing::trace!(
        alias = %alias.alias,
        agent_id = alias.agent_id,
        kind = %alias.kind,
        "agent alternate identity"
    );
}

fn log_alternate_identities(aliases: &[crate::store::persistence::agents::AgentAlias]) {
    for alias in aliases {
        log_one_alternate_identity(alias);
    }
    tracing::debug!(
        alias_count = aliases.len(),
        "agent alternate identities after disk scan"
    );
}

async fn seed_persona_aliases(
    persist: &PersistHandle,
    personas: &std::collections::BTreeMap<String, String>,
) -> Result<(), PersistError> {
    if personas.is_empty() {
        return Ok(());
    }
    let persona_names: Vec<String> = personas.keys().cloned().collect();
    let ids = persist.fetch_agent_ids_by_names(&persona_names).await?;
    upsert_resolved_personas(persist, personas, ids).await?;

    let aliases = persist.fetch_alias_map().await?;
    log_alternate_identities(&aliases);
    Ok(())
}

pub async fn scan_dirs(
    persist: &PersistHandle,
    targets: &[PathBuf],
) -> Result<usize, PersistError> {
    let (triples, personas) = collect_agent_files(targets).await;

    let by_name = dedupe_by_name(triples);
    if by_name.is_empty() {
        return Ok(0);
    }

    let total = by_name.len();
    let mut names: Vec<String> = Vec::with_capacity(total);
    let mut paths: Vec<String> = Vec::with_capacity(total);
    let mut descriptions: Vec<Option<String>> = Vec::with_capacity(total);
    for (name, (path, description)) in by_name {
        names.push(name);
        paths.push(path);
        descriptions.push(description);
    }

    let inserted = persist
        .bulk_upsert_file_backed_agents(&names, &paths, &descriptions)
        .await?;

    seed_persona_aliases(persist, &personas).await?;

    Ok(inserted as usize)
}

fn dedupe_by_name(
    triples: impl IntoIterator<Item = (String, String, Option<String>)>,
) -> std::collections::BTreeMap<String, (String, Option<String>)> {
    let mut by_name = std::collections::BTreeMap::new();
    for (name, path, description) in triples {
        if name.is_empty() {
            continue;
        }
        by_name.insert(name, (path, description));
    }
    by_name
}
