use super::project_scan::{
    ProjectSignals, find_working_directory_path, has_unsafe_path_chars,
    trusted_working_directory_sources,
};

pub(crate) fn extract_project(sig: ProjectSignals<'_>) -> Option<String> {
    if let Some(name) =
        extract_from_working_directory_marker(sig.system_prompt, &sig.message_sources)
    {
        return Some(name);
    }

    let mut all_sources: Vec<&str> = Vec::new();
    if let Some(sp) = sig.system_prompt {
        all_sources.push(sp);
    }
    all_sources.extend(sig.message_sources.iter().copied());

    if let Some(name) = extract_shortest_from_contents_of(&all_sources, true) {
        return Some(name);
    }

    extract_shortest_from_contents_of(&all_sources, false)
}

fn extract_from_working_directory_marker(
    system_prompt: Option<&str>,
    message_sources: &[&str],
) -> Option<String> {
    let trusted = trusted_working_directory_sources(system_prompt, message_sources);
    let path = find_working_directory_path(&trusted)?;
    let root = compute_project_root(path);
    root.split('/').find(|s| !s.is_empty())?;
    if has_unsafe_path_chars(root) {
        return None;
    }
    Some(root.to_string())
}

fn extract_shortest_from_contents_of(
    sources: &[&str],
    require_project_instructions: bool,
) -> Option<String> {
    let mut shortest_root: Option<String> = None;

    for source in sources {
        for line in source.lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with("Contents of /") {
                continue;
            }

            let after_prefix = &trimmed["Contents of ".len()..];
            let (path, annotation) = match after_prefix.find(" (") {
                Some(idx) => {
                    let after = &after_prefix[idx + 2..];
                    let anno = after.strip_suffix(':').unwrap_or(after);
                    let anno = anno.strip_suffix(')').unwrap_or(anno);
                    (&after_prefix[..idx], anno)
                }
                None => (after_prefix, ""),
            };

            if !path.contains("/CLAUDE.md") && !path.contains("/.claude/") {
                continue;
            }

            let root = compute_project_root(path);
            if has_unsafe_path_chars(root) {
                continue;
            }

            if require_project_instructions {
                if !annotation.contains("project instructions") {
                    continue;
                }
            } else {
                if annotation_is_user_global(annotation) {
                    continue;
                }
                let seg_count = root.split('/').filter(|s| !s.is_empty()).count();
                if seg_count <= 2 {
                    continue;
                }
            }

            let should_replace = match &shortest_root {
                Some(existing) => root.len() < existing.len(),
                None => true,
            };
            if should_replace {
                shortest_root = Some(root.to_string());
            }
        }
    }

    shortest_root
}

fn annotation_is_user_global(annotation: &str) -> bool {
    let lower = annotation.to_ascii_lowercase();
    lower.contains("user's private")
        || lower.contains("user's global")
        || lower.contains("user's auto-memory")
        || lower.contains("global instructions for all projects")
}

pub fn compute_project_root(path: &str) -> &str {
    let effective = match path.find("/.claude/worktrees/") {
        Some(idx) => &path[..idx],
        None => path,
    };

    if let Some(idx) = effective.find("/.claude/") {
        return &effective[..idx];
    }

    if let Some(idx) = effective.rfind("/CLAUDE.md") {
        return &effective[..idx];
    }

    effective
}

pub fn project_display_name(root: &str) -> String {
    let segments: Vec<&str> = root.split('/').filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        return String::new();
    }
    let skip = if segments.len() > 2 { 2 } else { 0 };
    let display: Vec<&str> = segments[skip..].to_vec();
    display.join("/")
}

pub(crate) fn legacy_trimmed_alias(root: &str) -> Option<String> {
    if !root.starts_with('/') {
        return None;
    }
    let trimmed = project_display_name(root);
    if trimmed.is_empty() || trimmed == root {
        return None;
    }
    Some(trimmed)
}

#[cfg(test)]
fn sig_from_prompt(prompt: &str) -> ProjectSignals<'_> {
    ProjectSignals {
        system_prompt: Some(prompt),
        message_sources: Vec::new(),
    }
}

#[cfg(test)]
#[path = "project_root_contents_of_tests.rs"]
mod contents_of_tests;
#[cfg(test)]
#[path = "project_root_display_tests.rs"]
mod display_tests;
