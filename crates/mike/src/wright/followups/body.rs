use crate::store::persistence::wright::FollowupRow;

pub(crate) const PAGE_URL: &str = "http://localhost:9990/wright";

pub(crate) fn marker(id: i64) -> String {
    format!("<!-- wright-followup:{id} -->")
}

fn neutral(text: &str) -> String {
    text.replace("<!--", "&lt;!--")
}

fn links(label: &str, anchor: &str, ids: &[i64]) -> String {
    if ids.is_empty() {
        return "none".to_owned();
    }
    ids.iter()
        .map(|id| format!("[{label} {id}]({PAGE_URL}#{anchor}-{id})"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn issue_title(row: &FollowupRow) -> String {
    neutral(row.title.trim())
}

pub(crate) fn issue_body(row: &FollowupRow) -> String {
    let check = row.check_name.as_deref().unwrap_or("none");
    let part = row.owning_part.as_deref().unwrap_or("none");
    [
        neutral(row.body.trim()),
        String::new(),
        "## Where Wright saw it".to_owned(),
        String::new(),
        format!("- Kind: `{}`", row.kind),
        format!("- Check: `{check}`"),
        format!("- Owning part: `{part}`"),
        format!("- Problems: {}", links("Problem", "problem", &row.problems)),
        format!("- Findings: {}", links("Finding", "finding", &row.findings)),
        format!(
            "- Register: follow-up {} on [the Wright page]({PAGE_URL})",
            row.id
        ),
        String::new(),
        marker(row.id),
    ]
    .join("\n")
}
