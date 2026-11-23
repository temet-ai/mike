use super::decide::Stmt;

pub fn has_paths_frontmatter(head: &str) -> bool {
    let Some(rest) = head.strip_prefix("---") else {
        return false;
    };
    let Some(end) = rest.find("\n---") else {
        return false;
    };
    rest[..end]
        .lines()
        .any(|line| line.trim_start().starts_with("paths:"))
}

pub fn is_always_loaded(
    source: Option<&str>,
    relative_path: Option<&str>,
    head: Option<&str>,
) -> bool {
    let (Some(source), Some(path)) = (source, relative_path) else {
        return false;
    };
    let base = path.rsplit('/').next().unwrap_or(path);
    match source {
        "instructions" => {
            if path.starts_with(".claude/agents/") {
                return false;
            }
            if matches!(base, "CLAUDE.md" | "AGENTS.md") {
                return true;
            }
            path.starts_with(".claude/rules/") && !has_paths_frontmatter(head.unwrap_or_default())
        }
        "rules" => !has_paths_frontmatter(head.unwrap_or_default()),
        _ => false,
    }
}

pub fn lesson_reached(loser: &Stmt, coverer: &Stmt) -> bool {
    if coverer.is_lesson() {
        return coverer.project_id == loser.project_id;
    }
    if !coverer.is_file() {
        return false;
    }
    if coverer.always_loaded {
        return coverer.machine_wide || coverer.project_id == loser.project_id;
    }
    coverer.source_name.as_deref() == Some("claude-memory") && coverer.project_id == loser.project_id
}

pub fn file_reached(loser: &Stmt, coverer: &Stmt) -> bool {
    if !coverer.is_file() {
        return false;
    }
    if coverer.item_id == loser.item_id {
        return true;
    }
    coverer.always_loaded
        && (coverer.machine_wide
            || (coverer.project_id.is_some() && coverer.project_id == loser.project_id))
}

pub fn reaches(loser: &Stmt, coverer: &Stmt) -> bool {
    if loser.is_lesson() {
        lesson_reached(loser, coverer)
    } else if loser.is_file() {
        file_reached(loser, coverer)
    } else {
        false
    }
}

pub fn not_reached_reason(loser: &Stmt, coverer: &Stmt) -> String {
    if loser.is_lesson() {
        format!(
            "Also said in {}, but the coverer reaches fewer sessions than this lesson",
            coverer.carrier
        )
    } else {
        format!(
            "also said in {}, which reaches different sessions",
            coverer.carrier
        )
    }
}

pub fn scopes_clash(a: &Stmt, b: &Stmt) -> bool {
    a.machine_wide != b.machine_wide
}
