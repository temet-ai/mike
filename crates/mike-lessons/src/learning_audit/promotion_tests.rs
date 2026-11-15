use serde_json::json;

use super::promotion::{PromotionState, publication_status};

#[test]
fn publication_status_missing_receipt_is_reported_as_missing() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let status = publication_status(directory.path())?;
    assert_eq!(status.state, "missing");
    assert_eq!(serde_json::to_value(&status)?, json!({"state": "missing"}));
    Ok(())
}

#[test]
fn publication_status_conflicts_and_clean_receipts_are_told_apart() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("latest.json");
    let mut receipt = json!({"lessons": 3, "memory_projects": 2,
                             "skills": ["codex/a", "claude/a"],
                             "conflicts": {"manual-skill": "preserved"}});
    std::fs::write(&path, serde_json::to_vec(&receipt)?)?;
    let status = publication_status(directory.path())?;
    assert_eq!(status.state, "conflict");
    assert_eq!(status.skill_installations, Some(2));
    assert_eq!(status.conflicts, Some(receipt["conflicts"].clone()));
    assert_eq!(status.lessons, Some(json!(3)));
    assert_eq!(status.memory_projects, Some(json!(2)));
    assert!(status.updated_at.is_some());
    receipt["conflicts"] = json!({});
    std::fs::write(&path, serde_json::to_vec(&receipt)?)?;
    assert_eq!(publication_status(directory.path())?.state, "published");
    Ok(())
}

#[test]
fn publication_status_unreadable_receipt_names_the_file() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    std::fs::write(directory.path().join("latest.json"), b"{not json")?;
    let Err(error) = publication_status(directory.path()) else {
        anyhow::bail!("a malformed receipt must not parse");
    };
    assert!(format!("{error:#}").contains("latest.json"));
    Ok(())
}

#[test]
fn promotion_state_reads_the_ledger_and_the_skill_names_in_the_manifest() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    std::fs::write(
        directory.path().join("skill-installs.json"),
        serde_json::to_vec(&json!({
            "u1:alpha": {"retired_at": null},
            "u1:old": {"retired_at": "2026-10-01"},
            "u2:delta": {"retired_at": ""},
        }))?,
    )?;
    std::fs::write(
        directory.path().join("manifest.json"),
        serde_json::to_vec(&json!({
            "/p/.claude/skills/shared-workflows/SKILL.md": "abc",
            "/p/artifacts/learning-memory/u1/memory.md": "def",
        }))?,
    )?;
    let state = PromotionState::load(directory.path())?;
    assert_eq!(
        state.manifest_names.iter().collect::<Vec<_>>(),
        ["shared-workflows"]
    );
    assert_eq!(
        state.live_skills_per_project(),
        [("u1".to_owned(), 1), ("u2".to_owned(), 1)]
    );
    assert!(
        PromotionState::load(&directory.path().join("absent"))?
            .ledger
            .is_empty()
    );
    Ok(())
}
