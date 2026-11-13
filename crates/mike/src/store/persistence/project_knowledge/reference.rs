use uuid::Uuid;

use super::invalid;
use crate::store::persistence::PersistError;

const UUID_KEYED_KINDS: [&str; 7] = [
    "lesson",
    "note",
    "thread",
    "question_answer",
    "mail",
    "learning",
    "session",
];
const MIN_PREFIX_CHARS: usize = 8;
const CANDIDATE_LIMIT: usize = 10;

pub(super) struct Settled {
    pub(super) kind: String,
    pub(super) key: String,
    pub(super) corrected: Option<String>,
}

enum Shape<'a> {
    Exact(&'a str, &'a str),
    Partial(Option<&'a str>, &'a str),
}

fn is_uuid_prefix(text: &str) -> bool {
    text.len() >= MIN_PREFIX_CHARS
        && text.len() <= 36
        && text.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
        && text.chars().next().is_some_and(|c| c.is_ascii_hexdigit())
}

fn shape<'a>(reference: &'a str, readable: &[&str]) -> Result<Shape<'a>, PersistError> {
    let Some((kind, key)) = reference.split_once(':') else {
        if reference.parse::<Uuid>().is_ok() || is_uuid_prefix(reference) {
            return Ok(Shape::Partial(None, reference));
        }
        return Err(invalid(&malformed_message(reference)));
    };
    if !readable.contains(&kind) {
        return Err(invalid(&format!(
            "Unknown knowledge source kind `{kind}` in `{reference}`; readable kinds are {}",
            readable.join(", ")
        )));
    }
    let names_uuid_item =
        UUID_KEYED_KINDS.contains(&kind) && key.parse::<Uuid>().is_err() && is_uuid_prefix(key);
    Ok(if names_uuid_item {
        Shape::Partial(Some(kind), key)
    } else {
        Shape::Exact(kind, key)
    })
}

fn reference_help() -> String {
    format!(
        "A reference looks like `note:<uuid>`, as search_project_knowledge returns it. A bare UUID or its first {MIN_PREFIX_CHARS} characters also works for a lesson, note, thread, question_answer, mail or learning."
    )
}

fn malformed_message(reference: &str) -> String {
    format!(
        "Invalid knowledge reference `{reference}`. {}",
        reference_help()
    )
}

fn no_match_message(reference: &str, project_id: Option<i64>) -> String {
    let scope = match project_id {
        Some(_) => " in this project",
        None => "",
    };
    format!(
        "No knowledge item matches `{reference}`{scope}. {}",
        reference_help()
    )
}

fn bounds(prefix: &str) -> Option<(Uuid, Uuid)> {
    let digits: String = prefix.chars().filter(|c| *c != '-').collect();
    if digits.len() > 32 {
        return None;
    }
    let low = Uuid::parse_str(&format!("{digits:0<32}")).ok()?;
    let high = Uuid::parse_str(&format!("{digits:f<32}")).ok()?;
    Some((low, high))
}

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub(super) async fn settle_reference(
        self,
        project_id: Option<i64>,
        reference: &str,
        readable: &[&str],
    ) -> Result<Settled, PersistError> {
        let (kind, key) = match shape(reference, readable)? {
            Shape::Exact(kind, key) => {
                return Ok(Settled {
                    kind: kind.to_owned(),
                    key: key.to_owned(),
                    corrected: None,
                });
            }
            Shape::Partial(kind, key) => (kind, key),
        };
        let mut found = self.reference_candidates(project_id, kind, key).await?;
        match found.len() {
            0 => Err(invalid(&no_match_message(reference, project_id))),
            1 => {
                let full = found.remove(0);
                let (kind, key) = full.split_once(':').unwrap_or((&full, ""));
                Ok(Settled {
                    kind: kind.to_owned(),
                    key: key.to_owned(),
                    corrected: Some(full.clone()),
                })
            }
            _ => Err(invalid(&ambiguous_message(reference, &found))),
        }
    }

    async fn reference_candidates(
        self,
        project_id: Option<i64>,
        kind: Option<&str>,
        prefix: &str,
    ) -> Result<Vec<String>, PersistError> {
        let Some((low, high)) = bounds(prefix) else {
            return Ok(Vec::new());
        };
        Ok(sqlx::query_file_scalar!(
            "src/store/persistence/project_knowledge/reference_candidates.sql",
            project_id,
            kind,
            low,
            high,
        )
        .fetch_all(self.pool())
        .await?)
    }
}

fn ambiguous_message(reference: &str, found: &[String]) -> String {
    let more = if found.len() >= CANDIDATE_LIMIT {
        " (the first 10 are listed)"
    } else {
        ""
    };
    format!(
        "Knowledge reference `{reference}` is ambiguous. It matches: {}{more}. Pass one of these as the reference.",
        found.join(", ")
    )
}
