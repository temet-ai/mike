#[derive(Debug, Default)]
pub struct Frontmatter {
    fields: Vec<(String, String)>,
}

pub struct Split<'a> {
    pub frontmatter: Frontmatter,
    pub body: &'a str,
    pub error: Option<String>,
}

impl Frontmatter {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
            .filter(|value| !value.trim().is_empty())
    }

    pub fn first_of(&self, keys: &[&str]) -> Option<&str> {
        keys.iter().find_map(|key| self.get(key))
    }
}

pub fn strip_quotes(value: &str) -> &str {
    let value = value.trim();
    for quote in ['"', '\''] {
        if let Some(inner) = value
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
        {
            return inner.trim();
        }
    }
    value
}

pub fn as_list(value: &str) -> Vec<String> {
    let value = value.trim();
    let inner = value
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'));
    let parts: Vec<&str> = match inner {
        Some(inner) => inner.split(',').collect(),
        None => value.lines().collect(),
    };
    parts
        .into_iter()
        .map(|part| strip_quotes(part.trim().trim_start_matches("- ")).to_owned())
        .filter(|part| !part.is_empty())
        .collect()
}

fn key_of(line: &str) -> Option<(String, String)> {
    let (name, rest) = line.split_once(':')?;
    let name = name.trim();
    let valid = !name.is_empty()
        && name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | ' '));
    valid.then(|| {
        let value = rest.trim();
        let value = if matches!(value, ">" | "|" | ">-" | "|-") {
            ""
        } else {
            value
        };
        (
            name.to_ascii_lowercase().replace([' ', '-'], "_"),
            value.to_owned(),
        )
    })
}

fn parse_lines<'a>(lines: impl Iterator<Item = &'a str>) -> Frontmatter {
    let mut fields: Vec<(String, String)> = Vec::new();
    for line in lines {
        let indented = line.starts_with([' ', '\t']) || line.trim_start().starts_with("- ");
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        if !indented && let Some((name, value)) = key_of(line) {
            fields.push((name, strip_quotes(&value).to_owned()));
            continue;
        }
        if let Some((_, value)) = fields.last_mut() {
            if !value.is_empty() {
                value.push('\n');
            }
            value.push_str(line.trim());
        }
    }
    Frontmatter { fields }
}

pub fn split(text: &str) -> Split<'_> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.split_inclusive('\n');
    let opens = lines.next().is_some_and(|line| line.trim() == "---");
    if !opens {
        return Split {
            frontmatter: Frontmatter::default(),
            body: text,
            error: None,
        };
    }
    let mut offset = text.find('\n').map_or(text.len(), |index| index + 1);
    let mut block = Vec::new();
    for line in lines {
        offset += line.len();
        if matches!(line.trim(), "---" | "...") {
            return Split {
                frontmatter: parse_lines(block.into_iter()),
                body: &text[offset..],
                error: None,
            };
        }
        block.push(line.trim_end_matches(['\n', '\r']));
    }
    Split {
        frontmatter: Frontmatter::default(),
        body: text,
        error: Some("frontmatter opens with `---` on line 1 but never closes".to_owned()),
    }
}
