use serde_json::{Map, Value};

const DELIMITER: &str = "---";

pub(super) fn parse(text: &str) -> Map<String, Value> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.first().map(|line| line.trim_end()) != Some(DELIMITER) {
        return Map::new();
    }
    let Some(offset) = lines
        .iter()
        .skip(1)
        .position(|line| line.trim_end() == DELIMITER)
    else {
        return error_map("unclosed frontmatter");
    };
    match entries(&lines[1..=offset]) {
        Ok(map) => map,
        Err(message) => error_map(&message),
    }
}

fn error_map(message: &str) -> Map<String, Value> {
    let mut map = Map::new();
    map.insert(
        "frontmatter_error".to_owned(),
        Value::String(message.to_owned()),
    );
    map
}

struct Pending<'a> {
    key: String,
    rest: String,
    children: Vec<&'a str>,
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

fn entries(lines: &[&str]) -> Result<Map<String, Value>, String> {
    let mut map = Map::new();
    let mut current: Option<Pending> = None;
    for (number, line) in lines.iter().copied().enumerate() {
        if line.trim().is_empty() {
            if let Some(pending) = current.as_mut() {
                pending.children.push("");
            }
            continue;
        }
        if indent(line) > 0 {
            let Some(pending) = current.as_mut() else {
                return Err(format!("unexpected indentation on line {}", number + 1));
            };
            pending.children.push(line);
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        if let Some(done) = current.take() {
            insert(&mut map, done)?;
        }
        let (key, rest) = line
            .split_once(':')
            .ok_or_else(|| format!("line {} is not 'key: value'", number + 1))?;
        current = Some(Pending {
            key: key.trim().to_owned(),
            rest: rest.trim().to_owned(),
            children: Vec::new(),
        });
    }
    if let Some(done) = current {
        insert(&mut map, done)?;
    }
    Ok(map)
}

fn insert(map: &mut Map<String, Value>, pending: Pending<'_>) -> Result<(), String> {
    let value = value_of(&pending.rest, &pending.children)?;
    map.insert(pending.key, value);
    Ok(())
}

fn joined(children: &[&str], separator: &str) -> String {
    children
        .iter()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(separator)
}

fn value_of(rest: &str, children: &[&str]) -> Result<Value, String> {
    if matches!(rest, ">" | ">-" | ">+" | "|" | "|-" | "|+") {
        let separator = if rest.starts_with('>') { " " } else { "\n" };
        return Ok(Value::String(joined(children, separator)));
    }
    if !rest.is_empty() {
        let mut text = unquote(rest);
        let tail = joined(children, " ");
        if !tail.is_empty() {
            text.push(' ');
            text.push_str(&tail);
        }
        return Ok(Value::String(text));
    }
    let present: Vec<&str> = children
        .iter()
        .copied()
        .filter(|line| !line.trim().is_empty())
        .collect();
    if present.is_empty() {
        return Ok(Value::String(String::new()));
    }
    if present
        .iter()
        .all(|line| line.trim_start().starts_with("- "))
    {
        return Ok(Value::Array(
            present
                .iter()
                .map(|line| Value::String(unquote(line.trim_start().trim_start_matches("- "))))
                .collect(),
        ));
    }
    let margin = present.iter().map(|line| indent(line)).min().unwrap_or(0);
    let dedented: Vec<&str> = children
        .iter()
        .map(|line| line.get(margin..).unwrap_or(""))
        .collect();
    entries(&dedented).map(Value::Object)
}

fn unquote(raw: &str) -> String {
    let text = raw.trim();
    let inner = text
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| {
            text.strip_prefix('\'')
                .and_then(|rest| rest.strip_suffix('\''))
        });
    inner.unwrap_or(text).to_owned()
}
