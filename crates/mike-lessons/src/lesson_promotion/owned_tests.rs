use std::fs;

use super::*;

fn count_archived(state: &Path) -> usize {
    fs::read_dir(state.join("retired")).map_or(0, |entries| entries.count())
}

#[test]
fn a_conflict_preserves_siblings_and_retirement_preserves_manual_edits() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let state = dir.path().join("state");
    let mut paths: Vec<PathBuf> = Vec::new();
    for client in ["codex", "claude"] {
        for leaf in ["SKILL.md", "references/memory.md"] {
            paths.push(dir.path().join(client).join(leaf));
        }
    }
    let desired: BTreeMap<String, String> = paths
        .iter()
        .map(|path| (path.display().to_string(), "generated".to_owned()))
        .collect();
    let mut manifest = Manifest::new();
    assert!(reconcile(&desired, &mut manifest, &state, true)?.is_empty());
    fs::write(&paths[1], "my edited memory")?;
    let conflicts = reconcile(&desired, &mut manifest, &state, true)?;
    assert!(conflicts.contains_key(&paths[1].display().to_string()));
    assert!(paths.iter().all(|path| path.exists()));
    let retired = reconcile(&BTreeMap::new(), &mut manifest, &state, true)?;
    assert!(retired.contains_key(&paths[1].display().to_string()));
    assert_eq!(fs::read_to_string(&paths[1])?, "my edited memory");
    assert!(
        paths
            .iter()
            .enumerate()
            .all(|(index, path)| index == 1 || !path.exists())
    );
    assert_eq!(count_archived(&state), 3);
    Ok(())
}

#[test]
fn a_retired_file_takes_its_emptied_skill_directory_with_it() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let skill = dir.path().join("skills/tool-old/SKILL.md");
    atomic_write(&skill, "generated")?;
    let mut manifest = Manifest::from([(skill.display().to_string(), sha256_hex(b"generated"))]);
    let conflicts = reconcile(
        &BTreeMap::new(),
        &mut manifest,
        &dir.path().join("state"),
        true,
    )?;
    assert!(conflicts.is_empty());
    assert!(!skill.parent().is_some_and(Path::exists));
    assert!(manifest.is_empty());
    Ok(())
}

#[test]
fn a_dry_run_changes_no_file_but_still_reports_conflicts() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let state = dir.path().join("state");
    let fresh = dir.path().join("fresh.md");
    let foreign = dir.path().join("foreign.md");
    fs::write(&foreign, "mine")?;
    let desired = BTreeMap::from([
        (fresh.display().to_string(), "generated".to_owned()),
        (foreign.display().to_string(), "generated".to_owned()),
    ]);
    let conflicts = reconcile(&desired, &mut Manifest::new(), &state, false)?;
    assert!(!fresh.exists());
    assert_eq!(fs::read_to_string(&foreign)?, "mine");
    assert_eq!(conflicts.len(), 1);
    assert!(!state.exists());
    Ok(())
}

#[test]
fn a_foreign_file_with_the_same_text_is_adopted_and_a_different_one_is_a_conflict()
-> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let same = dir.path().join("same.md");
    let different = dir.path().join("different.md");
    fs::write(&same, "body")?;
    fs::write(&different, "other")?;
    let mut manifest = Manifest::new();
    assert!(matches!(
        write_owned(&same, "body", &mut manifest, true)?,
        Owned::Adopted
    ));
    assert_eq!(manifest[&same.display().to_string()], sha256_hex(b"body"));
    assert!(matches!(
        write_owned(&different, "body", &mut manifest, true)?,
        Owned::Conflict(_)
    ));
    assert_eq!(fs::read_to_string(&different)?, "other");
    Ok(())
}

#[test]
fn an_owned_file_is_rewritten_when_the_body_changes() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("a/SKILL.md");
    let mut manifest = Manifest::new();
    assert!(matches!(
        write_owned(&path, "one", &mut manifest, true)?,
        Owned::Written
    ));
    assert!(matches!(
        write_owned(&path, "two", &mut manifest, true)?,
        Owned::Written
    ));
    assert_eq!(fs::read_to_string(&path)?, "two");
    assert_eq!(manifest[&path.display().to_string()], sha256_hex(b"two"));
    Ok(())
}

#[test]
fn exclude_lines_are_added_once_and_existing_lines_stay() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let target = dir.path().join("info/exclude");
    atomic_write(&target, "*.log")?;
    let lines = vec![
        ".claude/skills/lint/".to_owned(),
        ".claude/skills/lint/".to_owned(),
        ".claude/rules/mike-lessons.md".to_owned(),
    ];
    add_exclude_lines(&target, &lines)?;
    add_exclude_lines(&target, &lines)?;
    let text = fs::read_to_string(&target)?;
    assert_eq!(
        text.lines().collect::<Vec<_>>(),
        vec![
            "*.log",
            ".claude/skills/lint/",
            ".claude/rules/mike-lessons.md"
        ]
    );
    assert!(text.ends_with('\n'));
    Ok(())
}

#[test]
fn the_exclude_file_of_a_plain_checkout_and_of_a_linked_worktree() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let plain = dir.path().join("plain");
    fs::create_dir_all(plain.join(".git"))?;
    assert_eq!(exclude_file(&plain), Some(plain.join(".git/info/exclude")));

    let main = dir.path().join("main");
    let linked = dir.path().join("linked");
    fs::create_dir_all(main.join(".git/worktrees/linked"))?;
    fs::create_dir_all(&linked)?;
    fs::write(
        linked.join(".git"),
        format!("gitdir: {}\n", main.join(".git/worktrees/linked").display()),
    )?;
    fs::write(main.join(".git/worktrees/linked/commondir"), "../..\n")?;
    assert_eq!(
        exclude_file(&linked),
        Some(main.join(".git").canonicalize()?.join("info/exclude"))
    );
    assert_eq!(exclude_file(&dir.path().join("missing")), None);
    Ok(())
}
