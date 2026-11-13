#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
use serde_json::json;
use uuid::Uuid;

use super::commit::{CommitRequest, Ownership, choose_paths, commit, covers, owner_of, ownership};
use super::commit_verify::Verifier;
use super::definition::FleetDefinition;
use super::push::will_not_pass_on_a_retry;
use super::seated::Seated;

#[path = "commit_cases_tests.rs"]
pub mod cases;
#[path = "commit_modules_cases_tests.rs"]
pub mod modules_cases;
#[path = "commit_stub_tests.rs"]
pub mod stub_cases;
#[path = "commit_sweep_tests.rs"]
pub mod sweep_cases;
use super::tests::{complete, parsed};
use super::watch::git;

pub const NO_DB: Verifier<'static> = Verifier {
    persist: None,
    run: "fleet-test",
};

fn owner() -> Ownership {
    Ownership {
        task: "P1".into(),
        seat: "db-dev-P1".into(),
        owned: vec!["crates/mike/src/store/persistence/improvement".into()],
        shared: vec!["crates/mike/src/store/persistence/mod.rs".into()],
        foreign: vec!["crates/mike/src/store/persistence/launch".into()],
    }
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

#[test]
fn a_session_finds_its_task_and_its_files() {
    let session = Uuid::from_u128(5);
    let def = parsed(&complete());
    let seated = Seated::default().worker("P1", session, "db-dev-P1");
    let found = ownership(&def, &seated, session).unwrap();
    assert_eq!(found.task, "P1");
    assert_eq!(found.seat, "db-dev-P1");
    assert_eq!(
        found.owned,
        strings(&[
            "crates/mike/src/store/persistence/improvement",
            "crates/mike/src/store/persistence/mod.rs"
        ])
    );
    assert!(ownership(&def, &seated, Uuid::from_u128(6)).is_none());
    assert!(covers(
        &found.owned,
        "crates/mike/src/store/persistence/improvement/actions.rs"
    ));
    assert!(!covers(
        &found.owned,
        "crates/mike/src/store/persistence/improvements.rs"
    ));
    let (task, seat) = owner_of(&def, "frontend/src/routes/improvement/+page.svelte").unwrap();
    assert_eq!((task.as_str(), seat), ("P2", "page"));
}

#[test]
fn without_a_list_the_commit_takes_every_changed_file_of_the_task_and_nothing_else() {
    let changed = strings(&[
        "crates/mike/src/store/persistence/improvement/a.rs",
        "crates/mike/src/store/persistence/mod.rs",
        "frontend/src/routes/improvement/+page.svelte",
    ]);
    assert_eq!(
        choose_paths(&owner(), &changed, &[]).unwrap().paths,
        strings(&["crates/mike/src/store/persistence/improvement/a.rs"])
    );
    let none = choose_paths(&owner(), &strings(&["frontend/x.ts"]), &[]).unwrap_err();
    assert!(none.contains("nothing has changed under task P1's files"));
}

#[test]
fn a_named_path_is_committed_whoever_owns_it_and_the_report_says_so() {
    let changed = strings(&[
        "crates/mike/src/store/persistence/improvement/a.rs",
        "crates/mike/src/store/persistence/mod.rs",
        "frontend/src/routes/improvement/+page.svelte",
    ]);
    let shared = choose_paths(
        &owner(),
        &changed,
        &strings(&["crates/mike/src/store/persistence/mod.rs"]),
    )
    .unwrap();
    assert_eq!(
        shared.paths,
        strings(&["crates/mike/src/store/persistence/mod.rs"])
    );
    assert!(shared.warnings.is_empty());

    let foreign = choose_paths(
        &owner(),
        &changed,
        &strings(&["frontend/src/routes/improvement/+page.svelte"]),
    )
    .unwrap();
    assert_eq!(
        foreign.paths,
        strings(&["frontend/src/routes/improvement/+page.svelte"])
    );
    assert!(foreign.warnings[0].contains("not one of task P1's files"));

    let mixed = choose_paths(
        &owner(),
        &changed,
        &strings(&[
            "crates/mike/src/store/persistence/improvement/a.rs",
            "crates/mike/src/store/persistence/improvement/b.rs",
        ]),
    )
    .unwrap();
    assert_eq!(
        mixed.paths,
        strings(&["crates/mike/src/store/persistence/improvement/a.rs"])
    );
    assert!(mixed.warnings[0].contains("has not changed"));

    let untouched = choose_paths(
        &owner(),
        &changed,
        &strings(&["crates/mike/src/store/persistence/improvement/b.rs"]),
    )
    .unwrap_err();
    assert!(untouched.contains("none of the named paths has changed"));
}

#[test]
fn a_push_without_credentials_is_not_tried_again_and_a_ref_race_is() {
    assert!(will_not_pass_on_a_retry(
        "fatal: could not read Username for 'https://github.com': No such device or address"
    ));
    assert!(!will_not_pass_on_a_retry(
        "error: cannot lock ref 'refs/remotes/origin/main': is at 3b494e4 but expected 840d738"
    ));
    assert!(!will_not_pass_on_a_retry(
        "remote: fatal: remote unpack failed"
    ));
}

pub async fn scratch_repo() -> anyhow::Result<(tempfile::TempDir, String)> {
    let dir = tempfile::tempdir()?;
    let repo = dir.path().to_string_lossy().into_owned();
    for args in [
        vec!["init", "-q", "-b", "main"],
        vec!["config", "user.email", "t@example.com"],
        vec!["config", "user.name", "t"],
    ] {
        git(&repo, &args, None)
            .await
            .ok_or_else(|| anyhow::anyhow!("git {args:?}"))?;
    }
    std::fs::create_dir_all(dir.path().join("mine"))?;
    std::fs::create_dir_all(dir.path().join("theirs"))?;
    std::fs::write(dir.path().join("mine/a.txt"), "one\n")?;
    std::fs::write(dir.path().join("theirs/b.txt"), "one\n")?;
    git(&repo, &["add", "."], None).await;
    git(&repo, &["commit", "-q", "-m", "base"], None).await;
    Ok((dir, repo))
}

pub fn scratch_def(repo: &str) -> FleetDefinition {
    let mut value = complete();
    value["repo"] = json!(repo);
    value["tasks"] = json!({
        "M": {"name": "mine", "discipline": "db-dev", "criteria": ["C1"], "files": ["mine"],
              "brief": "x"},
        "T": {"name": "theirs", "discipline": "db-dev", "criteria": ["C2"], "files": ["theirs"],
              "brief": "x"}
    });
    value["shared_files"] = json!([]);
    parsed(&value)
}

pub fn scratch_seated(session: Uuid) -> Seated {
    Seated::default().worker("M", session, "db-dev-M")
}

#[tokio::test]
async fn a_seat_commits_its_own_changed_files_and_a_peers_staged_work_stays_out()
-> anyhow::Result<()> {
    let (dir, repo) = scratch_repo().await?;
    let session = Uuid::from_u128(11);
    let def = scratch_def(&repo);
    std::fs::write(dir.path().join("mine/a.txt"), "two\n")?;
    std::fs::write(dir.path().join("mine/new.txt"), "new\n")?;
    std::fs::write(dir.path().join("theirs/b.txt"), "peer edit\n")?;
    git(&repo, &["add", "theirs/b.txt"], None).await;
    let report = commit(
        &def,
        &scratch_seated(session),
        &CommitRequest {
            session,
            message: "feat: my slice".into(),
            paths: vec![],
            allow_empty: false,
        },
        NO_DB,
    )
    .await;
    assert_eq!(report.refused, None, "{report:?}");
    assert_eq!(report.paths, vec!["mine/a.txt", "mine/new.txt"]);
    assert!(!report.pushed, "a scratch repository has no origin");
    let shown = String::from_utf8(
        git(&repo, &["show", "--stat", "--format=%B", "HEAD"], None)
            .await
            .unwrap(),
    )?;
    assert!(shown.contains("feat: my slice") && shown.contains("Fleet-Seat: db-dev-M"));
    assert!(shown.contains("mine/a.txt") && shown.contains("mine/new.txt"));
    assert!(
        !shown.contains("theirs/b.txt"),
        "the peer's staged file was not swept in"
    );
    let status = String::from_utf8(git(&repo, &["status", "--porcelain"], None).await.unwrap())?;
    assert_eq!(
        status.trim(),
        "M  theirs/b.txt",
        "the peer's staging is untouched and mine reads clean"
    );
    let again = commit(
        &def,
        &scratch_seated(session),
        &CommitRequest {
            session,
            message: "x".into(),
            paths: vec![],
            allow_empty: false,
        },
        NO_DB,
    )
    .await;
    assert!(again.refused.unwrap().contains("nothing has changed"));
    Ok(())
}

#[tokio::test]
async fn a_message_with_no_file_change_lands_only_when_an_empty_commit_is_asked_for()
-> anyhow::Result<()> {
    let (_dir, repo) = scratch_repo().await?;
    let session = Uuid::from_u128(13);
    let def = scratch_def(&repo);
    let ask = |allow_empty| CommitRequest {
        session,
        message: "chore: the trial import is done\n\nCloses #60".into(),
        paths: vec![],
        allow_empty,
    };
    let before = git(&repo, &["rev-parse", "HEAD^{tree}"], None).await;

    let refused = commit(&def, &scratch_seated(session), &ask(false), NO_DB).await;
    assert!(refused.sha.is_none());
    assert!(refused.refused.unwrap().contains("nothing has changed"));

    let landed = commit(&def, &scratch_seated(session), &ask(true), NO_DB).await;
    assert_eq!(landed.refused, None);
    assert!(landed.sha.is_some());
    assert!(landed.paths.is_empty());
    assert_eq!(
        git(&repo, &["rev-parse", "HEAD^{tree}"], None).await,
        before
    );
    let message = git(&repo, &["log", "-1", "--format=%B"], None)
        .await
        .unwrap();
    let message = String::from_utf8_lossy(&message);
    assert!(message.contains("Refs #60"));
    assert!(!message.contains("Closes #60"));
    Ok(())
}

#[tokio::test]
async fn a_change_that_names_a_peers_uncommitted_file_is_committed_all_the_same()
-> anyhow::Result<()> {
    let (dir, repo) = scratch_repo().await?;
    let session = Uuid::from_u128(12);
    let def = scratch_def(&repo);
    std::fs::write(dir.path().join("theirs/helper.rs"), "pub fn help() {}\n")?;
    std::fs::write(dir.path().join("mine/a.txt"), "use theirs::helper::help;\n")?;
    let report = commit(
        &def,
        &scratch_seated(session),
        &CommitRequest {
            session,
            message: "x".into(),
            paths: vec![],
            allow_empty: false,
        },
        NO_DB,
    )
    .await;
    assert_eq!(report.refused, None);
    assert!(report.sha.is_some());
    assert_eq!(report.paths, strings(&["mine/a.txt"]));
    assert_eq!(report.repo, repo);
    assert!(
        !report.pushed && report.push_error.is_some(),
        "a scratch repository has no origin, and the report carries git's own words"
    );
    Ok(())
}

#[test]
fn closing_words_become_refs_and_the_issue_numbers_are_kept() {
    let (said, issues) = super::commit::rewrite_closing(
        "feat: x\n\nCloses #531, fixes #12 and #7.\nResolved: #3\nRefs #99",
    );
    assert_eq!(
        said,
        "feat: x\n\nRefs #531, Refs #12 and #7.\nRefs: #3\nRefs #99"
    );
    assert_eq!(issues, vec![3, 7, 12, 531]);
}

#[test]
fn a_message_with_no_closing_word_is_returned_as_it_was() {
    let message = "fix(fleet): the closes of a door\n\nRefs #4\n";
    let (said, issues) = super::commit::rewrite_closing(message);
    assert_eq!(said, message);
    assert!(issues.is_empty());
}
