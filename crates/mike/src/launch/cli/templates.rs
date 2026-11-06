use chrono::Utc;

use super::runner::Runner;
use crate::launch::channel_link;
use crate::launch::error::LaunchError;
use crate::store::persistence::launch::TemplateRow;

pub(crate) async fn list(runner: &Runner<'_>) -> Result<Vec<TemplateRow>, LaunchError> {
    runner
        .ledger
        .list_templates()
        .await
        .map_err(|e| LaunchError::Failed(format!("could not list templates: {e}")))
}

pub(crate) async fn show(runner: &Runner<'_>, name: &str) -> Result<TemplateRow, LaunchError> {
    runner
        .ledger
        .find_template_by_name(name)
        .await
        .map_err(|e| LaunchError::Failed(format!("could not look up template `{name}`: {e}")))?
        .ok_or_else(|| LaunchError::Refused(format!("no live template named `{name}`")))
}

pub(crate) async fn delete(
    runner: &Runner<'_>,
    name: &str,
    yes: bool,
) -> Result<bool, LaunchError> {
    if !yes {
        return Err(LaunchError::Usage(format!(
            "templates delete needs --yes to confirm closing `{name}`"
        )));
    }
    runner
        .ledger
        .close_template(name)
        .await
        .map_err(|e| LaunchError::Failed(format!("could not close template `{name}`: {e}")))
}

pub(crate) fn transcript_exists(template: &TemplateRow) -> bool {
    let Some(home) = std::env::var_os("HOME").filter(|h| !h.is_empty()) else {
        return false;
    };
    let folder = std::path::Path::new(&template.folder);
    channel_link::transcript_path(std::path::Path::new(&home), folder, template.session_uuid)
        .exists()
}

pub(crate) fn age(template: &TemplateRow) -> String {
    let seconds = Utc::now()
        .signed_duration_since(template.created_at)
        .num_seconds()
        .max(0);
    match seconds {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m", s / 60),
        s if s < 86_400 => format!("{}h", s / 3600),
        s => format!("{}d", s / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    fn row(created_at: chrono::DateTime<Utc>) -> TemplateRow {
        TemplateRow {
            id: 1,
            name: "verifier".into(),
            session_uuid: Uuid::new_v4(),
            folder: "/tmp/does-not-exist-mike-test".into(),
            backend: "herdr".into(),
            model: None,
            agent: None,
            claude_args: serde_json::json!([]),
            created_at,
            superseded_by: None,
            closed_at: None,
        }
    }

    #[test]
    fn age_buckets_seconds_minutes_hours_days() {
        assert_eq!(age(&row(Utc::now())), "0s");
        assert_eq!(age(&row(Utc::now() - chrono::Duration::seconds(90))), "1m");
        assert_eq!(age(&row(Utc::now() - chrono::Duration::hours(2))), "2h");
        assert_eq!(age(&row(Utc::now() - chrono::Duration::days(3))), "3d");
    }

    #[test]
    fn transcript_exists_false_for_a_folder_with_no_transcript() {
        assert!(!transcript_exists(&row(Utc::now())));
    }
}
