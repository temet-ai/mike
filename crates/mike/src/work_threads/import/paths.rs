pub(super) fn is_none_like(value: &str) -> bool {
    let trimmed = value
        .trim()
        .trim_matches(|c: char| matches!(c, '`' | '"' | '\'' | '(' | ')' | '.' | '*' | '_'))
        .trim()
        .to_ascii_lowercase();
    matches!(
        trimmed.as_str(),
        "" | "null" | "~" | "n/a" | "na" | "-" | "\u{2014}"
    ) || trimmed == "none"
        || trimmed.starts_with("none ")
        || trimmed.starts_with("none\u{2014}")
}

pub(super) fn first_token(value: &str) -> Option<String> {
    if is_none_like(value) {
        return None;
    }
    let value = value.trim();
    let token = match value
        .strip_prefix('`')
        .and_then(|rest| rest.split('`').next())
    {
        Some(inside) => inside.trim(),
        None => value.split_whitespace().next().unwrap_or_default(),
    };
    let token = token.trim_matches(|c: char| matches!(c, '`' | '"' | '\'' | ',' | ';' | '*'));
    (!token.is_empty() && !is_none_like(token)).then(|| token.to_owned())
}

pub(super) fn relativize(path: &str, roots: &[String]) -> Option<String> {
    let path = path.trim().trim_end_matches('/');
    let path = path.strip_prefix("./").unwrap_or(path);
    let mut ordered: Vec<&String> = roots.iter().collect();
    ordered.sort_by_key(|root| std::cmp::Reverse(root.len()));
    for root in ordered {
        let root = root.trim_end_matches('/');
        if root.is_empty() {
            continue;
        }
        if path == root {
            return Some(String::new());
        }
        if let Some(rest) = path
            .strip_prefix(root)
            .and_then(|rest| rest.strip_prefix('/'))
        {
            return Some(rest.to_owned());
        }
    }
    let absolute = path.starts_with(['/', '~']) || path.as_bytes().get(1) == Some(&b':');
    (!absolute && !path.is_empty() && !path.split('/').any(|part| part == ".."))
        .then(|| path.to_owned())
}

pub(super) fn looks_like_path(token: &str) -> bool {
    if token.contains(char::is_whitespace) || token.is_empty() {
        return false;
    }
    let has_extension = token.rsplit_once('.').is_some_and(|(stem, ext)| {
        !stem.is_empty()
            && (1..=6).contains(&ext.len())
            && ext.chars().all(|c| c.is_ascii_alphanumeric())
    });
    token.contains('/') || has_extension
}
