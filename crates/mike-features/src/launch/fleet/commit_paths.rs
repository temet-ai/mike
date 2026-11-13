use crate::store::persistence::fleet_repo::FleetPersist as _;
use uuid::Uuid;

use super::checks::normal;
use super::commit_verify::Verifier;
use super::definition::FleetDefinition;
use super::seated::Seated;
use crate::change_record::writers::change_record_enabled;

const LEFT_OUT_SHOWN: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ownership {
    pub task: String,
    pub seat: String,
    pub owned: Vec<String>,
    pub shared: Vec<String>,
    pub foreign: Vec<String>,
}

pub fn ownership(def: &FleetDefinition, seated: &Seated, session: Uuid) -> Option<Ownership> {
    let seat = seated.seat(session)?;
    let id = seat.task.as_deref()?;
    let task = def.tasks.get(id)?;
    let shared: Vec<String> = def.shared_files.iter().map(|f| normal(f)).collect();
    let foreign = def
        .tasks
        .iter()
        .filter(|(other, _)| other.as_str() != id)
        .flat_map(|(_, t)| t.files.iter().map(|f| normal(f)))
        .filter(|f| !shared.contains(f))
        .collect();
    Some(Ownership {
        task: id.to_string(),
        seat: seat.name.clone(),
        owned: task.files.iter().map(|f| normal(f)).collect(),
        shared,
        foreign,
    })
}

pub fn covers(prefixes: &[String], path: &str) -> bool {
    prefixes
        .iter()
        .any(|p| path == p || path.starts_with(&format!("{p}/")))
}

pub fn owner_of<'a>(def: &'a FleetDefinition, path: &str) -> Option<(&'a String, &'a str)> {
    if let Some(producer) = super::checks_paths::producer_for(def, path)
        && let Some((id, task)) = def.tasks.get_key_value(producer)
    {
        return Some((id, task.name.as_str()));
    }
    def.tasks.iter().find_map(|(id, t)| {
        let files: Vec<String> = t.files.iter().map(|f| normal(f)).collect();
        covers(&files, path).then_some((id, t.name.as_str()))
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chosen {
    pub paths: Vec<String>,
    pub warnings: Vec<String>,
}

pub async fn record_paths(
    verifier: Verifier<'_>,
    def: &FleetDefinition,
    session: Uuid,
) -> Option<Vec<String>> {
    let persist = verifier.persist?;
    if !change_record_enabled(persist).await {
        return None;
    }
    let identity = persist.session_identity_by_uuid(session).await.ok()??;
    let open = persist.fleet().open_changes(&def.repo).await.ok()?;
    let mut paths: Vec<String> = open
        .into_iter()
        .filter(|o| o.session_id == Some(identity.id))
        .map(|o| o.path)
        .collect();
    paths.sort();
    paths.dedup();
    (!paths.is_empty()).then_some(paths)
}

pub fn choose_from_record(
    owner: &Ownership,
    changed: &[String],
    recorded: &[String],
) -> Option<Chosen> {
    let mut chosen = Chosen {
        paths: Vec::new(),
        warnings: Vec::new(),
    };
    for path in recorded.iter().filter(|p| changed.contains(p)) {
        automatic_file(owner, path.clone(), &mut chosen);
    }
    (!chosen.paths.is_empty() || !chosen.warnings.is_empty()).then_some(chosen)
}

pub fn choose_paths(
    owner: &Ownership,
    changed: &[String],
    asked: &[String],
) -> Result<Chosen, String> {
    if asked.is_empty() {
        return choose_task_paths(owner, changed);
    }
    let mut chosen = Chosen {
        paths: Vec::new(),
        warnings: Vec::new(),
    };
    for path in asked {
        let path = normal(path);
        if changed.contains(&path) {
            named_file(owner, path, &mut chosen);
        } else {
            named_folder(owner, changed, &path, &mut chosen);
        }
    }
    if chosen.paths.is_empty() {
        return Err(format!(
            "none of the named paths has changed: {}",
            asked.join(", ")
        ));
    }
    Ok(chosen)
}

fn choose_task_paths(owner: &Ownership, changed: &[String]) -> Result<Chosen, String> {
    let mut chosen = Chosen {
        paths: Vec::new(),
        warnings: Vec::new(),
    };
    for path in changed.iter().filter(|p| covers(&owner.owned, p)) {
        automatic_file(owner, path.clone(), &mut chosen);
    }
    if chosen.paths.is_empty() {
        let why = format!(
            "nothing has changed under task {}'s files ({})",
            owner.task,
            owner.owned.join(", ")
        );
        return Err(std::iter::once(why)
            .chain(chosen.warnings)
            .collect::<Vec<_>>()
            .join("; "));
    }
    Ok(chosen)
}

fn automatic_file(owner: &Ownership, path: String, chosen: &mut Chosen) {
    if covers(&owner.foreign, &path) && !covers(&owner.shared, &path) {
        chosen.warnings.push(format!(
            "`{path}` belongs to another task and is left uncommitted; name the file explicitly to commit it"
        ));
        return;
    }
    named_file(owner, path, chosen);
}

fn named_file(owner: &Ownership, path: String, chosen: &mut Chosen) {
    if !covers(&owner.owned, &path) && !covers(&owner.shared, &path) {
        chosen.warnings.push(format!(
            "`{path}` is not one of task {}'s files and not a shared file",
            owner.task
        ));
    }
    if covers(&owner.owned, &path) && covers(&owner.foreign, &path) && !covers(&owner.shared, &path)
    {
        chosen.warnings.push(format!(
            "`{path}` also belongs to another task; it is committed because the file was named explicitly"
        ));
    }
    if !chosen.paths.contains(&path) {
        chosen.paths.push(path);
    }
}

fn named_folder(owner: &Ownership, changed: &[String], folder: &str, chosen: &mut Chosen) {
    let under: Vec<&String> = changed
        .iter()
        .filter(|file| file.starts_with(&format!("{folder}/")))
        .collect();
    if under.is_empty() {
        chosen.warnings.push(format!(
            "`{folder}` has not changed, so it is not in the commit"
        ));
        return;
    }
    let (theirs, taken): (Vec<&String>, Vec<&String>) = under
        .into_iter()
        .partition(|file| covers(&owner.foreign, file) && !covers(&owner.shared, file));
    for file in taken {
        if !chosen.paths.contains(file) {
            chosen.paths.push(file.clone());
        }
    }
    if !theirs.is_empty() {
        chosen.warnings.push(left_out(folder, &theirs));
    }
}

fn left_out(folder: &str, theirs: &[&String]) -> String {
    let shown: Vec<&str> = theirs
        .iter()
        .take(LEFT_OUT_SHOWN)
        .map(|file| file.as_str())
        .collect();
    let more = theirs.len().saturating_sub(LEFT_OUT_SHOWN);
    format!(
        "`{folder}` is a folder; {} changed file(s) under it belong to another task and are not in the commit ({}{}). Name a file itself to commit it",
        theirs.len(),
        shown.join(", "),
        if more > 0 {
            format!(", and {more} more")
        } else {
            String::new()
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owner() -> Ownership {
        Ownership {
            task: "T07".into(),
            seat: "client-developer-T07".into(),
            owned: vec!["crates/client".into()],
            shared: vec![],
            foreign: vec!["crates/sim".into()],
        }
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    fn changed() -> Vec<String> {
        strings(&[
            "crates/client/a.rs",
            "crates/client/ui/b.rs",
            "crates/sim/c.rs",
            "tools/shot/README.md",
            "tools/shot/pixels.py",
        ])
    }

    #[test]
    fn the_record_leaves_another_tasks_paths_uncommitted_and_reports_them() {
        let recorded = strings(&["crates/client/a.rs", "crates/sim/c.rs", "gone.rs"]);
        let Some(chosen) = choose_from_record(&owner(), &changed(), &recorded) else {
            panic!("recorded paths that changed are selected");
        };
        assert_eq!(chosen.paths, strings(&["crates/client/a.rs"]));
        assert_eq!(chosen.warnings.len(), 1);
        assert!(chosen.warnings[0].contains("`crates/sim/c.rs` belongs to another task"));
        assert!(chosen.warnings[0].contains("left uncommitted"));
        assert!(choose_from_record(&owner(), &changed(), &strings(&["gone.rs"])).is_none());
    }

    #[test]
    fn a_record_containing_only_foreign_paths_preserves_the_exclusion_warning() {
        let Some(chosen) = choose_from_record(&owner(), &changed(), &strings(&["crates/sim/c.rs"]))
        else {
            panic!("the exclusion must be reported even when nothing is selected");
        };
        assert!(chosen.paths.is_empty());
        assert_eq!(chosen.warnings.len(), 1);
        assert!(chosen.warnings[0].contains("left uncommitted"));
    }

    #[test]
    fn a_record_keeps_shared_and_unclaimed_paths_attributed_to_the_caller() {
        let mut owner = owner();
        owner.shared.push("crates/sim/c.rs".into());
        let recorded = strings(&["crates/sim/c.rs", "tools/shot/pixels.py"]);
        let Some(chosen) = choose_from_record(&owner, &changed(), &recorded) else {
            panic!("shared and unclaimed recorded paths remain selectable");
        };
        assert_eq!(chosen.paths, recorded);
        assert_eq!(chosen.warnings.len(), 1);
        assert!(chosen.warnings[0].contains("tools/shot/pixels.py"));
    }

    #[test]
    fn automatic_selection_respects_foreign_subdirectories_of_an_owned_folder() {
        let mut owner = owner();
        owner.owned = strings(&["crates"]);
        let Ok(chosen) = choose_paths(&owner, &changed(), &[]) else {
            panic!("the nonoverlapping task files are selected");
        };
        assert_eq!(
            chosen.paths,
            strings(&["crates/client/a.rs", "crates/client/ui/b.rs"])
        );
        assert!(chosen.warnings[0].contains("crates/sim/c.rs"));
        let blocked = choose_paths(&owner, &strings(&["crates/sim/c.rs"]), &[]);
        assert!(blocked.is_err_and(|why| why.contains("left uncommitted")));
    }

    #[test]
    fn an_overlapping_foreign_path_requires_naming_the_file_itself() {
        let mut owner = owner();
        owner.owned = strings(&["crates"]);
        let Ok(folder) = choose_paths(&owner, &changed(), &strings(&["crates"])) else {
            panic!("a named parent selects only nonforeign files");
        };
        assert!(!folder.paths.contains(&"crates/sim/c.rs".to_string()));
        let Ok(file) = choose_paths(&owner, &changed(), &strings(&["crates/sim/c.rs"])) else {
            panic!("an explicitly named file overrides the conflicting claim");
        };
        assert_eq!(file.paths, strings(&["crates/sim/c.rs"]));
        assert!(file.warnings[0].contains("also belongs to another task"));
    }

    #[test]
    fn a_snapshot_regeneration_attributed_to_a_peer_cannot_be_swept_into_its_commit() {
        let mut owner = owner();
        owner.foreign.push("tests/snapshots".into());
        let mut recorded = strings(&["crates/client/a.rs"]);
        recorded.extend((0..50).map(|n| format!("tests/snapshots/mix_{n}.snap")));
        let Some(chosen) = choose_from_record(&owner, &recorded, &recorded) else {
            panic!("the caller's own source file is selected");
        };
        assert_eq!(chosen.paths, strings(&["crates/client/a.rs"]));
        assert_eq!(chosen.warnings.len(), 50);
        for n in 0..50 {
            assert!(chosen.warnings[n].contains(&format!("tests/snapshots/mix_{n}.snap")));
            assert!(chosen.warnings[n].contains("left uncommitted"));
        }
    }

    #[test]
    fn a_named_folder_takes_every_changed_file_under_it() {
        let Ok(new_folder) = choose_paths(&owner(), &changed(), &strings(&["tools/shot/"])) else {
            panic!("a folder of new files is committed by naming it");
        };
        assert_eq!(
            new_folder.paths,
            strings(&["tools/shot/README.md", "tools/shot/pixels.py"])
        );
        assert!(new_folder.warnings.is_empty(), "{:?}", new_folder.warnings);
    }

    #[test]
    fn a_named_folder_leaves_out_what_belongs_to_another_task_and_says_so() {
        let Ok(wide) = choose_paths(&owner(), &changed(), &strings(&["crates"])) else {
            panic!("the seat's own files under the folder are committed");
        };
        assert_eq!(
            wide.paths,
            strings(&["crates/client/a.rs", "crates/client/ui/b.rs"])
        );
        assert_eq!(wide.warnings.len(), 1);
        assert!(
            wide.warnings[0].starts_with(
                "`crates` is a folder; 1 changed file(s) under it belong to another task and are not in the commit (crates/sim/c.rs)"
            ),
            "{}",
            wide.warnings[0]
        );
        let Ok(named) = choose_paths(&owner(), &changed(), &strings(&["crates/sim/c.rs"])) else {
            panic!("a file named itself is committed whoever owns it");
        };
        assert_eq!(named.paths, strings(&["crates/sim/c.rs"]));
        assert!(named.warnings[0].contains("not one of task T07's files"));
    }

    #[test]
    fn a_folder_with_nothing_changed_under_it_is_not_a_commit() {
        let nothing = choose_paths(&owner(), &changed(), &strings(&["docs/"]));
        assert_eq!(
            nothing,
            Err("none of the named paths has changed: docs/".to_string())
        );
        let Ok(both) = choose_paths(
            &owner(),
            &changed(),
            &strings(&["crates/client/a.rs", "crates/client"]),
        ) else {
            panic!("a file and its folder named together are one commit");
        };
        assert_eq!(
            both.paths,
            strings(&["crates/client/a.rs", "crates/client/ui/b.rs"])
        );
    }
}
