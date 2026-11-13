use super::WorkThreadError;

pub const ALLOWED_STATUSES: [&str; 5] = ["active", "implementing", "review", "parked", "done"];
pub const WRITABLE_KINDS: [&str; 6] = [
    "checkpoint",
    "decision",
    "finding",
    "brief",
    "handoff",
    "note",
];
pub const TLDR_LIMIT: usize = 600;
pub const CONTRIBUTION_LIMIT: usize = 300;
const SLUG_LIMIT: usize = 120;

pub fn invalid(message: String) -> WorkThreadError {
    WorkThreadError::Invalid(message)
}

pub fn slug(value: &str) -> Result<&str, WorkThreadError> {
    let slug = value.trim();
    if slug.is_empty() {
        return Err(invalid(
            "slug is empty; name the thread, for example `frontend-motion`".into(),
        ));
    }
    if slug.chars().count() > SLUG_LIMIT {
        return Err(invalid(format!(
            "slug `{slug}` is {} characters; the limit is {SLUG_LIMIT}",
            slug.chars().count()
        )));
    }
    let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-');
    if let Some((position, bad)) = slug.chars().enumerate().find(|(_, c)| !allowed(*c)) {
        return Err(invalid(format!(
            "slug `{slug}` has the character {bad:?} at position {position} (counting from 0); use letters, digits, '.', '_' and '-'"
        )));
    }
    if !slug.starts_with(|c: char| c.is_ascii_alphanumeric()) {
        return Err(invalid(format!(
            "slug `{slug}` must start with a letter or digit, found {:?} at position 0",
            slug.chars().next().unwrap_or(' ')
        )));
    }
    Ok(slug)
}

pub fn status(value: &str) -> Result<&str, WorkThreadError> {
    let status = value.trim();
    if ALLOWED_STATUSES.contains(&status) {
        return Ok(status);
    }
    Err(invalid(format!(
        "status `{value}` is not one of {}",
        ALLOWED_STATUSES.join(", ")
    )))
}

pub fn status_filter(raw: &str) -> Result<Vec<String>, WorkThreadError> {
    raw.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| {
            let known =
                ALLOWED_STATUSES.contains(&part) || matches!(part, "complete" | "in-progress");
            known.then(|| part.to_owned()).ok_or_else(|| {
                invalid(format!(
                    "status filter `{part}` is not one of {}; separate several with commas",
                    ALLOWED_STATUSES.join(", ")
                ))
            })
        })
        .collect()
}

pub fn kind(value: &str) -> Result<&str, WorkThreadError> {
    let kind = value.trim();
    if WRITABLE_KINDS.contains(&kind) {
        return Ok(kind);
    }
    Err(invalid(format!(
        "kind `{value}` is not one of {}; `state-replaced` entries are written by checkpoints only",
        WRITABLE_KINDS.join(", ")
    )))
}

pub fn tldr(value: &str) -> Result<&str, WorkThreadError> {
    let tldr = value.trim();
    if tldr.is_empty() {
        return Err(invalid(
            "tldr is empty; give the one-paragraph summary of where the thread stands".into(),
        ));
    }
    let length = tldr.chars().count();
    if length > TLDR_LIMIT {
        return Err(invalid(format!(
            "tldr is {length} characters; the limit is {TLDR_LIMIT} ({} too many). Shorten it and put the detail in where_we_are",
            length - TLDR_LIMIT
        )));
    }
    Ok(tldr)
}

pub fn non_blank<'a>(label: &str, value: &'a str) -> Result<&'a str, WorkThreadError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(invalid(format!("{label} is empty")));
    }
    Ok(trimmed)
}

pub fn relative_path<'a>(label: &str, value: &'a str) -> Result<&'a str, WorkThreadError> {
    let path = value.trim();
    if path.is_empty() {
        return Err(invalid(format!("{label} is empty")));
    }
    let drive =
        path.len() >= 2 && path.as_bytes()[0].is_ascii_alphabetic() && path.as_bytes()[1] == b':';
    if path.starts_with(['/', '\\', '~']) || drive {
        return Err(invalid(format!(
            "{label} `{path}` is an absolute path; give it relative to the project root, for example `crates/mike/src/main.rs`"
        )));
    }
    if let Some(index) = path.split(['/', '\\']).position(|part| part == "..") {
        return Err(invalid(format!(
            "{label} `{path}` climbs out of the project root at path segment {index} (counting from 0)"
        )));
    }
    Ok(path)
}

pub fn relative_paths(label: &str, values: &[String]) -> Result<Vec<String>, WorkThreadError> {
    values
        .iter()
        .enumerate()
        .map(|(index, value)| relative_path(&format!("{label}[{index}]"), value).map(str::to_owned))
        .collect()
}

pub fn first_line(body: &str) -> String {
    body.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default()
        .to_owned()
}

pub fn contribution(value: &str) -> Result<&str, WorkThreadError> {
    let length = value.chars().count();
    if length > CONTRIBUTION_LIMIT {
        return Err(invalid(format!(
            "contribution is {length} characters; the limit is {CONTRIBUTION_LIMIT}. It is one line saying what this session did"
        )));
    }
    Ok(value.trim())
}
