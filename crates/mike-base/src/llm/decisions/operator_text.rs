use crate::wire::machine_text;

const BLOCK_TAGS: [&str; 4] = [
    "system-reminder",
    "task-notification",
    "channel",
    "local-command-",
];

const MACHINE_PREFIXES: [&str; 9] = [
    "[Workflow harness",
    "Base directory for this skill:",
    "<bash-input>",
    "<bash-stdout>",
    "<command-name>",
    "<command-message>",
    "Mail from ",
    "Another Claude session sent a message",
    "\u{2713} built in",
];

pub fn typed_by_operator(text: &str) -> Option<String> {
    let kept = strip_blocks(text);
    let trimmed = kept.trim();
    if trimmed.is_empty() || is_machine_text(trimmed) {
        return None;
    }
    Some(trimmed.to_owned())
}

fn is_machine_text(text: &str) -> bool {
    MACHINE_PREFIXES.iter().any(|p| text.starts_with(p))
        || is_slash_command(text)
        || is_skill_name(text)
        || is_image_placeholders(text)
        || machine_text::classify(text).is_some()
}

fn is_slash_command(text: &str) -> bool {
    let Some(rest) = text.strip_prefix('/') else {
        return false;
    };
    let name = rest.split_whitespace().next().unwrap_or_default();
    !text.contains('\n')
        && !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':'))
}

fn is_skill_name(text: &str) -> bool {
    let mut words = text.split('-');
    let lower_word = |w: &str| {
        !w.is_empty()
            && w.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    };
    text.contains('-') && words.all(lower_word)
}

fn is_image_placeholders(text: &str) -> bool {
    text.split_whitespace()
        .collect::<String>()
        .split_inclusive(']')
        .all(|part| {
            part.strip_prefix("[Image#")
                .and_then(|p| p.strip_suffix(']'))
                .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        })
}

fn strip_blocks(text: &str) -> String {
    let mut kept = String::with_capacity(text.len());
    let mut at = 0;
    while let Some((start, end)) = next_block(text, at) {
        kept.push_str(&text[at..start]);
        at = end;
    }
    kept.push_str(&text[at..]);
    kept
}

fn next_block(text: &str, from: usize) -> Option<(usize, usize)> {
    let mut search = from;
    while let Some(offset) = text[search..].find('<') {
        let start = search + offset;
        if let Some(end) = block_end(text, start) {
            return Some((start, end));
        }
        search = start + 1;
    }
    None
}

fn tag_matches(name: &str, tag: &str) -> bool {
    if tag.ends_with('-') {
        name.len() > tag.len() && name.starts_with(tag)
    } else {
        name == tag
    }
}

fn block_end(text: &str, start: usize) -> Option<usize> {
    let rest = &text[start + 1..];
    let name_len = rest
        .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
        .unwrap_or(rest.len());
    let name = &rest[..name_len];
    if !BLOCK_TAGS.iter().any(|tag| tag_matches(name, tag)) {
        return None;
    }
    let open_end = start + 1 + rest.find('>')? + 1;
    if text[..open_end].ends_with("/>") {
        return Some(open_end);
    }
    let close = format!("</{name}>");
    Some(
        text[open_end..]
            .find(&close)
            .map_or(text.len(), |at| open_end + at + close.len()),
    )
}
