use std::path::{Component, Path};

use serde::Deserialize;
use serde_json::Value;

use crate::store::persistence::wright::NewFinding;

pub(crate) const FINDING_FILE: &str = "finding.json";
const MAX_FILE_BYTES: usize = 256 * 1024;
const MAX_TEXT: usize = 16 * 1024;
const MAX_EVIDENCE: usize = 100;
const MAX_REF: usize = 1024;
const KINDS: [&str; 5] = ["code_defect", "config", "leftover", "external", "unknown"];
const EVIDENCE_KINDS: [&str; 6] = ["request", "problem", "episode", "file", "log", "session"];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvidenceRef {
    kind: String,
    #[serde(rename = "ref")]
    reference: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FindingDoc {
    symptom: String,
    suspected_cause: String,
    confirmed_cause: Option<String>,
    cause_established: bool,
    owning_part: Option<String>,
    kind: String,
    remedy: String,
    evidence: Vec<EvidenceRef>,
}

fn text(field: &str, value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{field} is empty"));
    }
    if trimmed.len() > MAX_TEXT {
        return Err(format!(
            "{field} is {} bytes; at most {MAX_TEXT} are kept",
            trimmed.len()
        ));
    }
    Ok(trimmed.to_owned())
}

fn optional(field: &str, value: Option<&str>) -> Result<Option<String>, String> {
    value
        .filter(|v| !v.trim().is_empty())
        .map(|v| text(field, v))
        .transpose()
}

fn owning_part(part: &str, repo: &Path) -> Result<String, String> {
    let path = Path::new(part);
    let relative = path
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    if !relative {
        return Err(format!(
            "owning_part `{part}` must be a path inside the repository, without `..` or a leading `/`"
        ));
    }
    if !repo.join(path).exists() {
        return Err(format!(
            "owning_part `{part}` does not exist in the repository"
        ));
    }
    Ok(part.trim_start_matches("./").to_owned())
}

fn evidence(items: &[EvidenceRef]) -> Result<Value, String> {
    if items.is_empty() || items.len() > MAX_EVIDENCE {
        return Err(format!(
            "evidence has {} items; it needs 1 to {MAX_EVIDENCE}",
            items.len()
        ));
    }
    let mut out = Vec::with_capacity(items.len());
    for (n, item) in items.iter().enumerate() {
        if !EVIDENCE_KINDS.contains(&item.kind.as_str()) {
            return Err(format!(
                "evidence[{n}].kind is `{}`; it must be one of {}",
                item.kind,
                EVIDENCE_KINDS.join(", ")
            ));
        }
        let reference = item.reference.trim();
        if reference.is_empty() || reference.len() > MAX_REF {
            return Err(format!("evidence[{n}].ref must be 1 to {MAX_REF} bytes"));
        }
        out.push(serde_json::json!({"kind": item.kind, "ref": reference}));
    }
    Ok(Value::Array(out))
}

fn cause(doc: &FindingDoc, repo: &Path) -> Result<(Option<String>, Option<String>), String> {
    let confirmed = optional("confirmed_cause", doc.confirmed_cause.as_deref())?;
    let part = match doc.owning_part.as_deref().filter(|p| !p.trim().is_empty()) {
        Some(part) => Some(owning_part(part.trim(), repo)?),
        None => None,
    };
    match (doc.cause_established, &confirmed, &part) {
        (true, None, _) => Err("cause_established is true but confirmed_cause is missing".to_owned()),
        (true, _, None) => Err("cause_established is true but owning_part is missing".to_owned()),
        (false, Some(_), _) => Err(
            "cause_established is false but confirmed_cause is set; leave it null when the cause was not established"
                .to_owned(),
        ),
        _ => Ok((confirmed, part)),
    }
}

pub(crate) fn validate(bytes: &[u8], repo: &Path) -> Result<NewFinding, String> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(format!(
            "{FINDING_FILE} is {} bytes; at most {MAX_FILE_BYTES} are read",
            bytes.len()
        ));
    }
    let doc: FindingDoc = serde_json::from_slice(bytes).map_err(|e| {
        format!(
            "{FINDING_FILE} is not a valid finding at line {}, column {}: {e}",
            e.line(),
            e.column()
        )
    })?;
    if !KINDS.contains(&doc.kind.as_str()) {
        return Err(format!(
            "kind is `{}`; it must be one of {}",
            doc.kind,
            KINDS.join(", ")
        ));
    }
    let (confirmed_cause, owning_part) = cause(&doc, repo)?;
    Ok(NewFinding {
        symptom: text("symptom", &doc.symptom)?,
        suspected_cause: text("suspected_cause", &doc.suspected_cause)?,
        confirmed_cause,
        cause_established: doc.cause_established,
        owning_part,
        kind: doc.kind,
        remedy: text("remedy", &doc.remedy)?,
        evidence: evidence(&doc.evidence)?,
    })
}

pub(crate) fn as_json(finding: &NewFinding) -> Value {
    serde_json::json!({
        "symptom": finding.symptom,
        "suspected_cause": finding.suspected_cause,
        "confirmed_cause": finding.confirmed_cause,
        "cause_established": finding.cause_established,
        "owning_part": finding.owning_part,
        "kind": finding.kind,
        "remedy": finding.remedy,
        "evidence": finding.evidence,
    })
}
