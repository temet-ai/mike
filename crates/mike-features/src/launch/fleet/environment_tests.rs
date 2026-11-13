use std::path::Path;

use super::definition::FleetDefinition;
use super::environment::{Environment, check_environment, repo_is_linked_worktree};
use super::push::git_said;

async fn said(dir: &Path, args: &[&str]) -> anyhow::Result<String> {
    git_said(&dir.to_string_lossy(), args)
        .await
        .map_err(|why| anyhow::anyhow!("git {args:?}: {why}"))
}

fn definition() -> FleetDefinition {
    FleetDefinition {
        repo: "/work/clone".into(),
        branch: "fleet/x".into(),
        ..FleetDefinition::default()
    }
}

fn repo_problems(env: &Environment) -> Vec<String> {
    check_environment(&definition(), env)
        .into_iter()
        .filter(|missing| missing.item == "repo")
        .map(|missing| missing.problem)
        .collect()
}

#[test]
fn a_linked_worktree_is_not_a_refusal() {
    let env = Environment {
        repo_exists: true,
        repo_worktree_of: Some("/srv/main".into()),
        ..Environment::default()
    };
    assert_eq!(repo_problems(&env), Vec::<String>::new());
}

#[test]
fn a_checkout_that_cannot_push_makes_the_fleet_not_ready_in_gits_words() {
    let env = Environment {
        repo_exists: true,
        push_problem: Some("fatal: could not read Username for 'https://github.com'".into()),
        ..Environment::default()
    };
    let problems = repo_problems(&env);
    assert_eq!(problems.len(), 1);
    assert!(
        problems[0].contains("`fleet/x` cannot be pushed"),
        "{problems:?}"
    );
    assert!(
        problems[0].contains("could not read Username"),
        "{problems:?}"
    );
}

#[test]
fn a_checkout_that_can_push_has_no_push_problem() {
    let env = Environment {
        repo_exists: true,
        ..Environment::default()
    };
    assert_eq!(repo_problems(&env), Vec::<String>::new());
}

#[tokio::test]
async fn a_linked_git_worktree_is_told_from_a_main_checkout() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let main = root.path().join("main");
    std::fs::create_dir_all(&main)?;
    said(&main, &["init", "-q", "-b", "main"]).await?;
    said(&main, &["config", "user.email", "t@example.com"]).await?;
    said(&main, &["config", "user.name", "t"]).await?;
    std::fs::write(main.join("a.txt"), "one\n")?;
    said(&main, &["add", "a.txt"]).await?;
    said(&main, &["commit", "-q", "-m", "a"]).await?;
    let linked = root.path().join("linked");
    said(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "fleet/x",
            &linked.to_string_lossy(),
        ],
    )
    .await?;
    assert!(repo_is_linked_worktree(&linked.to_string_lossy()).await);
    assert!(!repo_is_linked_worktree(&main.to_string_lossy()).await);
    Ok(())
}
