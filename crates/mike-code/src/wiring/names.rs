use std::borrow::Cow;
use std::sync::LazyLock;

use regex::Regex;

use super::spec::{Alias, Normalize};

static BRACES: LazyLock<Regex> = LazyLock::new(|| match Regex::new(r"\$?\{[^}]*\}") {
    Ok(re) => re,
    Err(e) => panic!("invalid static placeholder pattern: {e}"),
});

pub(super) struct DeclaredName {
    pub(super) name: String,
    pub(super) aliases: Vec<String>,
}

pub(super) fn declared(
    raw: &str,
    normalize: Normalize,
    aliases: &[Alias],
    source: &str,
) -> Option<DeclaredName> {
    let raw = raw.trim();
    let quoted = unquote(raw) != raw;
    let (name, mut extra) = if quoted || !aliases.contains(&Alias::Const) {
        (clean(raw, normalize), Vec::new())
    } else {
        (const_value(raw, source)?, vec![raw.to_owned()])
    };
    if aliases.contains(&Alias::Snake) {
        extra.push(snake(&name));
    }
    Some(DeclaredName {
        name,
        aliases: extra,
    })
}

pub(super) fn clean(raw: &str, normalize: Normalize) -> String {
    let unquoted = unquote(raw.trim());
    match normalize {
        Normalize::Plain => strip_generics(unquoted).to_owned(),
        Normalize::Route => route(unquoted),
    }
}

pub(super) fn prepare_line(line: &str, normalize: Normalize) -> Cow<'_, str> {
    match normalize {
        Normalize::Route => BRACES.replace_all(line, "{}"),
        Normalize::Plain => Cow::Borrowed(line),
    }
}

pub(super) fn is_comment_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    ["//", "#", "*", "/*", "<!--"]
        .iter()
        .any(|lead| trimmed.starts_with(lead))
}

fn unquote(raw: &str) -> &str {
    for quote in ['"', '\'', '`'] {
        if raw.len() >= 2 && raw.starts_with(quote) && raw.ends_with(quote) {
            return &raw[1..raw.len() - 1];
        }
    }
    raw
}

fn strip_generics(name: &str) -> &str {
    name.split('<').next().unwrap_or(name).trim()
}

fn route(raw: &str) -> String {
    let braced = BRACES.replace_all(raw, "{}");
    let mut trimmed: &str = braced.trim_end_matches(['.', ':', ',']);
    while let Some(rest) = trimmed.strip_suffix("{}") {
        if rest.ends_with('/') {
            break;
        }
        trimmed = rest;
    }
    trimmed.to_owned()
}

fn snake(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn const_value(ident: &str, source: &str) -> Option<String> {
    let pattern = format!(
        r#"const\s+{}\s*:\s*&(?:'static\s+)?str\s*=\s*"([^"]*)""#,
        regex::escape(ident)
    );
    let found = Regex::new(&pattern).ok()?;
    Some(found.captures(source)?.get(1)?.as_str().to_owned())
}
