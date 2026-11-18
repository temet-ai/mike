use serde_json::{Value, json};

pub use crate::workspace::checkouts::is_scratch;

const INDEX_FOLDER: &str = "/artifacts/learning-memory/";

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

pub fn nest_worktrees(mut projects: Vec<Value>) -> Vec<Value> {
    for project in &mut projects {
        let Some(locations) = project.get_mut("locations").and_then(Value::as_array_mut) else {
            continue;
        };
        let links: Vec<(Value, Value)> = locations
            .iter()
            .filter_map(|location| {
                let parent = location.get("worktree_of").filter(|v| !v.is_null())?;
                Some((parent.clone(), location.get("location_uuid")?.clone()))
            })
            .collect();
        for location in locations.iter_mut() {
            let own = location.get("location_uuid").cloned();
            let worktrees: Vec<Value> = links
                .iter()
                .filter(|(parent, _)| Some(parent) == own.as_ref())
                .map(|(_, child)| child.clone())
                .collect();
            if let Some(fields) = location.as_object_mut() {
                fields.insert("worktrees".to_owned(), Value::Array(worktrees));
            }
        }
    }
    projects
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
    fn nest_worktrees_lists_each_worktree_under_its_main_checkout() {
        let rows = vec![json!({"project_path": "/p", "locations": [
            {"location_uuid": "main", "worktree_of": null},
            {"location_uuid": "wt", "worktree_of": "main"},
        ]})];
        let nested = nest_worktrees(rows);
        assert_eq!(nested[0]["locations"][0]["worktrees"], json!(["wt"]));
        assert_eq!(nested[0]["locations"][1]["worktrees"], json!([]));
    }

    #[test]
    fn full_view_returns_every_row() {
        let rows = vec![project("/tmp/x", &["/tmp/x/artifacts/learning-memory/y"])];
        assert_eq!(full_view(rows.clone())["projects"], json!(rows));
    }
}
