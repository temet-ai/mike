use std::path::Path;

use uuid::Uuid;

use crate::store::persistence::fleet::FleetCheckoutUser;

use super::cleanup::{
    RUN_CLONE_MARKER, Verdict, clean_run, mark_run_clone, marker_names, offer_text, other_user_of,
    verdict,
};
use super::definition::FleetDefinition;
use super::gate_slot::{check_slot_for, slot_for};

async fn said(dir: &Path, args: &[&str]) -> anyhow::Result<String> {
    let output = tokio::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .await?;
    anyhow::ensure!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

struct Scene {
    _root: tempfile::TempDir,
    origin: std::path::PathBuf,
    clone: std::path::PathBuf,
}

async fn scene() -> anyhow::Result<Scene> {
    let root = tempfile::tempdir()?;
    let origin = root.path().join("origin");
    std::fs::create_dir_all(&origin)?;
    said(&origin, &["init", "-q", "-b", "main"]).await?;
    said(&origin, &["config", "user.email", "t@example.com"]).await?;
    said(&origin, &["config", "user.name", "t"]).await?;
    std::fs::write(origin.join("a.txt"), "one\n")?;
    said(&origin, &["add", "-A"]).await?;
    said(&origin, &["commit", "-q", "-m", "base"]).await?;
    let clones = root.path().join("worktrees");
    std::fs::create_dir_all(&clones)?;
    let clone = clones.join("project-run");
    said(
        &clones,
        &[
            "clone",
            "-q",
            &origin.to_string_lossy(),
            &clone.to_string_lossy(),
        ],
    )
    .await?;
    said(&clone, &["config", "user.email", "t@example.com"]).await?;
    said(&clone, &["config", "user.name", "t"]).await?;
    std::fs::write(clone.join(RUN_CLONE_MARKER), "my-fleet\n")?;
    Ok(Scene {
        _root: root,
        origin,
        clone,
    })
}

async fn add_slots(clone: &Path) -> anyhow::Result<[std::path::PathBuf; 2]> {
    let text = clone.to_string_lossy().into_owned();
    let gate = slot_for(&text).ok_or_else(|| anyhow::anyhow!("no slot"))?;
    let check = check_slot_for(&text).ok_or_else(|| anyhow::anyhow!("no slot"))?;
    super::gate_slot::exclude_slots(clone).await;
    std::fs::create_dir_all(clone.join(".claude/worktrees"))?;
    for slot in [&gate, &check] {
        said(
            clone,
            &[
                "worktree",
                "add",
                "-q",
                "--detach",
                &slot.to_string_lossy(),
                "HEAD",
            ],
        )
        .await?;
        std::fs::create_dir_all(slot.join("target/debug"))?;
        std::fs::write(slot.join("target/debug/big.rlib"), "built\n")?;
    }
    Ok([gate, check])
}

#[test]
fn the_marker_names_the_fleet_by_slug_or_run() {
    assert!(marker_names("my-fleet\n", "my-fleet", "fleet-my-fleet-1"));
    assert!(marker_names(
        "\nfleet-my-fleet-1",
        "my-fleet",
        "fleet-my-fleet-1"
    ));
    assert!(!marker_names("other\n", "my-fleet", "fleet-my-fleet-1"));
    assert!(!marker_names("", "my-fleet", "fleet-my-fleet-1"));
}

#[tokio::test]
async fn a_finished_run_clone_and_its_clean_checkouts_are_removed() -> anyhow::Result<()> {
    let scene = scene().await?;
    let slots = add_slots(&scene.clone).await?;
    let cleaned = clean_run(&scene.clone, "my-fleet", "run-1", None).await;
    assert_eq!(cleaned.kept, None);
    assert!(!scene.clone.exists());
    assert!(slots.iter().all(|slot| !slot.exists()));
    assert!(scene.origin.join("a.txt").exists());
    Ok(())
}

#[tokio::test]
async fn a_clone_with_uncommitted_work_is_kept_with_its_checkouts() -> anyhow::Result<()> {
    let scene = scene().await?;
    let slots = add_slots(&scene.clone).await?;
    std::fs::write(scene.clone.join("a.txt"), "edited\n")?;
    let cleaned = clean_run(&scene.clone, "my-fleet", "run-1", None).await;
    assert!(cleaned.kept.is_some_and(|why| why.contains("uncommitted")));
    assert!(scene.clone.exists());
    assert!(slots.iter().all(|slot| slot.exists()));
    Ok(())
}

#[tokio::test]
async fn a_clone_with_an_unpushed_commit_is_kept() -> anyhow::Result<()> {
    let scene = scene().await?;
    std::fs::write(scene.clone.join("b.txt"), "two\n")?;
    said(&scene.clone, &["add", "b.txt"]).await?;
    said(&scene.clone, &["commit", "-q", "-m", "local only"]).await?;
    let verdict = verdict(&scene.clone, "my-fleet", "run-1", None).await;
    assert!(matches!(verdict, Verdict::Keep(why) if why.contains("not pushed")));
    assert!(scene.clone.exists());
    Ok(())
}

#[tokio::test]
async fn a_clone_with_a_stash_is_kept() -> anyhow::Result<()> {
    let scene = scene().await?;
    std::fs::write(scene.clone.join("a.txt"), "stashed\n")?;
    said(&scene.clone, &["stash", "push", "-q"]).await?;
    let verdict = verdict(&scene.clone, "my-fleet", "run-1", None).await;
    assert!(matches!(verdict, Verdict::Keep(why) if why.contains("stash")));
    Ok(())
}

#[tokio::test]
async fn a_checkout_without_the_marker_or_for_another_run_is_never_touched() -> anyhow::Result<()> {
    let scene = scene().await?;
    let slots = add_slots(&scene.clone).await?;
    let other = clean_run(&scene.clone, "another-fleet", "run-2", None).await;
    assert!(other.kept.is_some());
    std::fs::remove_file(scene.clone.join(RUN_CLONE_MARKER))?;
    let none = clean_run(&scene.clone, "my-fleet", "run-1", None).await;
    assert!(none.kept.is_some_and(|why| why.contains("nothing says")));
    assert!(scene.clone.exists());
    assert!(slots.iter().all(|slot| slot.exists()));
    Ok(())
}

#[tokio::test]
async fn a_clone_another_unfinished_fleet_uses_is_kept() -> anyhow::Result<()> {
    let scene = scene().await?;
    let slots = add_slots(&scene.clone).await?;
    let cleaned = clean_run(&scene.clone, "my-fleet", "run-1", Some("next-fleet")).await;
    assert!(cleaned.kept.is_some_and(|why| why.contains("next-fleet")));
    assert!(scene.clone.exists());
    assert!(slots.iter().all(|slot| slot.exists()));
    Ok(())
}

#[tokio::test]
async fn an_ignored_local_file_keeps_the_clone() -> anyhow::Result<()> {
    let scene = scene().await?;
    std::fs::write(scene.clone.join(".gitignore"), ".env\n")?;
    said(&scene.clone, &["add", ".gitignore"]).await?;
    said(&scene.clone, &["commit", "-q", "-m", "ignore env"]).await?;
    said(
        &scene.clone,
        &["push", "-q", "origin", "HEAD:refs/heads/side"],
    )
    .await?;
    std::fs::write(scene.clone.join(".env"), "KEY=1\n")?;
    let cleaned = clean_run(&scene.clone, "my-fleet", "run-1", None).await;
    assert!(cleaned.kept.is_some_and(|why| why.contains("uncommitted")));
    assert!(scene.clone.join(".env").exists());
    Ok(())
}

#[test]
fn only_a_fleet_that_is_not_done_and_names_the_same_repo_counts_as_a_user() {
    let own = Uuid::from_u128(1);
    let user = |fleet_uuid: Uuid, slug: &str, state: &str, repo: &str| FleetCheckoutUser {
        fleet_uuid,
        slug: slug.to_owned(),
        state: state.to_owned(),
        repo: repo.to_owned(),
    };
    let fleets = vec![
        user(own, "mine", "running", "/w/c"),
        user(Uuid::from_u128(2), "old", "done", "/w/c"),
        user(Uuid::from_u128(3), "elsewhere", "running", "/w/d"),
        user(Uuid::from_u128(4), "next", "draft", "/w/c"),
    ];
    assert_eq!(other_user_of("/w/c", own, &fleets), Some("next"));
    assert_eq!(other_user_of("/w/e", own, &fleets), None);
}

#[tokio::test]
async fn marking_a_clone_lets_its_run_remove_it_and_the_project_checkout_is_never_marked()
-> anyhow::Result<()> {
    let scene = scene().await?;
    std::fs::remove_file(scene.clone.join(RUN_CLONE_MARKER))?;
    let def: FleetDefinition = serde_json::from_value(serde_json::json!({
        "repo": scene.clone.to_string_lossy()
    }))?;
    mark_run_clone(&def, &scene.clone, "my-fleet");
    assert!(!scene.clone.join(RUN_CLONE_MARKER).exists());
    mark_run_clone(&def, &scene.origin, "my-fleet");
    mark_run_clone(&def, &scene.origin, "my-fleet");
    let text = std::fs::read_to_string(scene.clone.join(RUN_CLONE_MARKER))?;
    assert_eq!(text, "my-fleet\n");
    let cleaned = clean_run(&scene.clone, "my-fleet", "run-1", None).await;
    assert_eq!(cleaned.kept, None);
    assert!(!scene.clone.exists());
    Ok(())
}

#[test]
fn the_offer_names_the_folders_and_the_reason() {
    let text = offer_text(
        "my-fleet",
        Path::new("/w/project-run"),
        "2 uncommitted paths",
    );
    assert!(text.contains("my-fleet") && text.contains("2 uncommitted paths"));
    assert!(
        text.contains("/w/project-run/.claude/worktrees/mike-gate")
            && text.contains("/w/project-run/.claude/worktrees/mike-check")
    );
}

#[tokio::test]
async fn a_linked_worktree_is_not_a_run_clone() -> anyhow::Result<()> {
    let scene = scene().await?;
    let slots = add_slots(&scene.clone).await?;
    std::fs::write(slots[0].join(RUN_CLONE_MARKER), "my-fleet\n")?;
    let verdict = verdict(&slots[0], "my-fleet", "run-1", None).await;
    assert!(matches!(verdict, Verdict::Keep(why) if why.contains("own .git")));
    Ok(())
}
