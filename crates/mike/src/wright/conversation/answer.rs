use serde::Serialize;
use serde_json::{Value, json};

pub(crate) const BODY_MAX_CHARS: usize = 32 * 1024;
pub(crate) const LINKS_MAX: usize = 50;
pub(crate) const NAMED_MAX: usize = 10;
pub(crate) const LINK_KINDS: [&str; 5] = ["problem", "episode", "action", "finding", "repair"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct Link {
    pub(crate) kind: String,
    pub(crate) id: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Answer {
    pub(crate) body: String,
    pub(crate) links: Vec<Link>,
    pub(crate) steer: Option<Value>,
    pub(crate) needs_decision: bool,
}

fn link(item: &Value) -> Option<Link> {
    let kind = item.get("kind")?.as_str()?;
    let id = item.get("id")?.as_i64()?;
    (LINK_KINDS.contains(&kind) && id >= 1).then(|| Link {
        kind: kind.to_owned(),
        id,
    })
}

pub(crate) fn parse(value: &Value) -> Result<Answer, String> {
    let Some(body) = value.get("body").and_then(Value::as_str).map(str::trim) else {
        return Err(format!(
            "answer.json has no text body: `body` is {}",
            value
                .get("body")
                .map_or_else(|| "missing".to_owned(), Value::to_string)
        ));
    };
    if body.is_empty() || body.chars().count() > BODY_MAX_CHARS {
        return Err(format!(
            "answer.json body has {} characters; it must have 1 to {BODY_MAX_CHARS}",
            body.chars().count()
        ));
    }
    let mut links: Vec<Link> = value
        .get("links")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(link).collect())
        .unwrap_or_default();
    links.dedup();
    links.truncate(LINKS_MAX);
    Ok(Answer {
        body: body.to_owned(),
        links,
        steer: value.get("steer").filter(|steer| !steer.is_null()).cloned(),
        needs_decision: value
            .get("needs_decision")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

pub(crate) fn keep_existing(links: Vec<Link>, existing: &[(String, i64)]) -> Vec<Link> {
    links
        .into_iter()
        .filter(|link| match link.kind.as_str() {
            "problem" | "episode" => existing
                .iter()
                .any(|(kind, id)| *kind == link.kind && *id == link.id),
            _ => true,
        })
        .collect()
}

pub(crate) fn ids_of(links: &[Link], kind: &str) -> Vec<i64> {
    links
        .iter()
        .filter(|link| link.kind == kind)
        .map(|link| link.id)
        .collect()
}

pub(crate) fn links_json(links: &[Link]) -> Value {
    json!(links)
}

fn digits_at(text: &str) -> Option<i64> {
    let digits: String = text.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok().filter(|id| *id >= 1)
}

pub(crate) fn named_problems(body: &str) -> Vec<i64> {
    let lower = body.to_lowercase();
    let mut named = Vec::new();
    for (at, _) in lower.match_indices('#') {
        named.extend(digits_at(&lower[at + 1..]));
    }
    for (at, word) in lower.match_indices("problem") {
        let rest = lower[at + word.len()..].trim_start_matches([' ', '#']);
        named.extend(digits_at(rest));
    }
    named.sort_unstable();
    named.dedup();
    named.truncate(NAMED_MAX);
    named
}
