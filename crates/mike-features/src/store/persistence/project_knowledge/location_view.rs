use serde_json::{Value, json};

const SCRATCH_PREFIXES: [&str; 5] = [
    "/tmp/",
    "/var/folders/",
    "/private/tmp/",
    "/private/var/folders/",
    "__",
];
const SCRATCH_PARTS: [&str; 3] = [
    "/.claude/.cache/tmp/",
    "/.local/state/mike/seats/",
    "{parent}",
];
const INDEX_FOLDER: &str = "/artifacts/learning-memory/";

pub fn is_scratch(path: &str) -> bool {
    SCRATCH_PREFIXES
        .iter()
        .any(|prefix| path.starts_with(prefix))
        || SCRATCH_PARTS.iter().any(|part| path.contains(part))
}

fn drop_index_locations(project: &mut Value) -> usize {
    let Some(locations) = project.get_mut("locations").and_then(Value::as_array_mut) else {
        return 0;
    };
    let before = locations.len();
    locations.retain(|location| {
        !location
            .get("root_path")
            .and_then(Value::as_str)
            .is_some_and(|path| path.contains(INDEX_FOLDER))
    });
    before - locations.len()
}

pub fn default_view(projects: Vec<Value>) -> Value {
    let total = projects.len();
    let mut hidden_locations = 0;
    let shown: Vec<Value> = projects
        .into_iter()
        .filter(|project| {
            !project
                .get("project_path")
                .and_then(Value::as_str)
                .is_some_and(is_scratch)
        })
        .map(|mut project| {
            hidden_locations += drop_index_locations(&mut project);
            project
        })
        .collect();
    json!({
        "projects": shown,
        "hidden_scratch_projects": total - shown.len(),
        "hidden_index_locations": hidden_locations,
        "note": "Scratch projects (under /tmp, seat and scratchpad folders) and learning-memory index locations are hidden; pass include_all=true for every row.",
    })
}

pub fn full_view(projects: Vec<Value>) -> Value {
    json!({"projects": projects})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(path: &str, roots: &[&str]) -> Value {
        let locations: Vec<Value> = roots
            .iter()
            .map(|root| json!({"root_path": root}))
            .collect();
        json!({"project_path": path, "locations": locations})
    }

    #[test]
    fn default_view_hides_scratch_projects_and_counts_them() {
        let view = default_view(vec![
            project("/mnt/data/projects/real", &[]),
            project("/tmp/fleet-ref-node.x/work", &[]),
            project("/home/u/.local/state/mike/seats/p/tmp/c4", &[]),
            project("__probe__", &[]),
        ]);
        assert_eq!(view["projects"].as_array().map(Vec::len), Some(1));
        assert_eq!(view["hidden_scratch_projects"], 3);
    }

    #[test]
    fn default_view_hides_index_locations_but_keeps_checkouts() {
        let view = default_view(vec![project(
            "/mnt/data/projects/real",
            &[
                "/mnt/data/projects/real",
                "/mnt/data/projects/mike/artifacts/learning-memory/abc",
            ],
        )]);
        let locations = view["projects"][0]["locations"].as_array().map(Vec::len);
        assert_eq!(locations, Some(1));
        assert_eq!(view["hidden_index_locations"], 1);
    }

    #[test]
    fn full_view_returns_every_row() {
        let rows = vec![project("/tmp/x", &["/tmp/x/artifacts/learning-memory/y"])];
        assert_eq!(full_view(rows.clone())["projects"], json!(rows));
    }
}
