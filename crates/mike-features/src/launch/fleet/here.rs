use std::path::Path;

use crate::store::persistence::fleet::FleetRow;

pub fn checkout_present(project_path: &str) -> bool {
    Path::new(project_path).is_dir()
}

pub fn on_this_machine(fleets: Vec<FleetRow>, present: impl Fn(&str) -> bool) -> Vec<FleetRow> {
    fleets
        .into_iter()
        .filter(|fleet| present(&fleet.project_path))
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    use super::*;

    fn fleet(slug: &str, project_path: &str) -> FleetRow {
        FleetRow {
            id: 1,
            fleet_uuid: Uuid::new_v4(),
            slug: slug.to_string(),
            project_path: project_path.to_string(),
            state: "running".to_string(),
            definition: json!({}),
            missing: json!([]),
            run_id: Some(format!("run-{slug}")),
            start_report: None,
            created_by: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            ready_at: None,
            started_at: None,
            finished_at: None,
        }
    }

    #[test]
    fn a_fleet_whose_checkout_is_on_another_machine_is_left_to_that_machine() {
        let fleets = vec![
            fleet("on-the-mac", "/Users/someone/projects/app"),
            fleet("on-this-host", "/srv/projects/app"),
        ];

        let kept = on_this_machine(fleets, |path| path == "/srv/projects/app");

        let slugs: Vec<&str> = kept.iter().map(|f| f.slug.as_str()).collect();
        assert_eq!(slugs, vec!["on-this-host"]);
    }

    #[test]
    fn a_folder_that_does_not_exist_here_is_not_present() {
        assert!(!checkout_present(
            "/no-such-folder-on-any-machine/mike-fleet-here-test"
        ));
        assert!(checkout_present(
            std::env::temp_dir().to_string_lossy().as_ref()
        ));
    }
}
