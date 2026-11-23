use std::path::PathBuf;

use chrono::Utc;
use uuid::Uuid;

use super::{Seen, marks};
use crate::store::persistence::location_lifecycle::CheckoutRow;
use crate::workspace::git_repo::RepoIdentity;

fn row(root: &str, parent: Option<Uuid>) -> CheckoutRow {
    CheckoutRow {
        location_uuid: Uuid::new_v4(),
        root_path: root.to_owned(),
        project_id: 1,
        project_uuid: Uuid::nil(),
        origin_url: None,
        parent_location_uuid: parent,
        present_at: None,
        created_at: Utc::now(),
    }
}

fn seen(checkout: &str, main: &str) -> Seen {
    Seen {
        present: true,
        parent_dir_present: true,
        identity: Some(RepoIdentity {
            checkout_root: PathBuf::from(checkout),
            main_root: PathBuf::from(main),
            origin_url: None,
        }),
        knowledge_project: None,
    }
}

#[test]
fn marks_leave_out_links_that_already_hold() {
    let main = row("/work/repo/", None);
    let worktree = row("/work/repo-wt", Some(main.location_uuid));
    let rows = [main, worktree];
    let seen = [
        seen("/work/repo", "/work/repo"),
        seen("/work/repo-wt", "/work/repo"),
    ];

    let marks = marks(&rows, &seen);

    assert_eq!(
        marks.locations.len(),
        2,
        "presence is still marked for every row"
    );
    assert!(marks.classified.is_empty());
}

#[test]
fn marks_carry_a_worktree_whose_parent_is_missing_or_wrong() {
    let main = row("/work/repo", None);
    let unlinked = row("/work/repo-a", None);
    let wrong = row("/work/repo-b", Some(Uuid::new_v4()));
    let stale_main = row("/work/other", Some(main.location_uuid));
    let rows = [main, unlinked.clone(), wrong.clone(), stale_main.clone()];
    let seen = [
        seen("/work/repo", "/work/repo"),
        seen("/work/repo-a", "/work/repo"),
        seen("/work/repo-b", "/work/repo"),
        seen("/work/other", "/work/other"),
    ];

    let marks = marks(&rows, &seen);

    assert_eq!(
        marks.classified,
        vec![
            unlinked.location_uuid,
            wrong.location_uuid,
            stale_main.location_uuid
        ]
    );
    assert_eq!(
        marks.main_roots,
        vec![
            Some("/work/repo".to_owned()),
            Some("/work/repo".to_owned()),
            None
        ]
    );
}
